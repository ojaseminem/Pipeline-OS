//! Classifies Git network failures so fetch/pull/push can retry the ones a
//! second attempt might fix (DNS hiccups, dropped connections, proxy and
//! gateway errors) while surfacing the ones it never will (bad credentials,
//! rejected pushes) immediately.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Total attempts for a network operation: the first try plus up to three
/// retries, waiting 2s, 4s, then 8s (with the default backoff).
pub const MAX_NETWORK_ATTEMPTS: u32 = 4;

pub(crate) const DEFAULT_RETRY_BACKOFF: Duration = Duration::from_secs(2);

/// Why a Git network command failed, as far as its stderr tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkFailure {
    /// Likely to succeed if tried again (connectivity, timeouts, 5xx).
    Transient,
    /// Credentials are missing, wrong, or lack access — retrying just
    /// hammers the server (and may lock the account).
    Auth,
    /// The remote rejected the push (non-fast-forward, protected branch,
    /// hook declined). Needs a pull or a policy change, not a retry.
    Rejected,
    /// Anything else (bad ref, local error…).
    Other,
}

const AUTH_PATTERNS: &[&str] = &[
    "authentication failed",
    "could not read username",
    "could not read password",
    "permission denied (publickey",
    "permission to ",
    "invalid username or password",
    "http basic: access denied",
    "terminal prompts disabled",
    "repository not found",
    "returned error: 401",
    "returned error: 403",
    "the requested url returned error: 401",
    "the requested url returned error: 403",
    "host key verification failed",
    "access denied",
    "bad credentials",
    "support for password authentication was removed",
];

/// Failures that look like network trouble but won't fix themselves.
const PERMANENT_PATTERNS: &[&str] = &[
    "ssl certificate problem",
    "certificate verify failed",
    "returned error: 404",
];

const REJECTED_PATTERNS: &[&str] = &[
    "[rejected]",
    "! [remote rejected]",
    "non-fast-forward",
    "fetch first",
    "updates were rejected",
    "pre-receive hook declined",
    "protected branch",
    "gh006",
    "gh013",
    "failed to push some refs",
];

const TRANSIENT_PATTERNS: &[&str] = &[
    "could not resolve host",
    "could not resolve proxy",
    "temporary failure in name resolution",
    "connection timed out",
    "operation timed out",
    "timed out after",
    "connection reset",
    "connection refused",
    "connection was reset",
    "connection closed",
    "early eof",
    "rpc failed",
    "the remote end hung up unexpectedly",
    "ssl_error",
    "ssl_connect",
    "gnutls",
    "schannel",
    "network is unreachable",
    "failed to connect",
    "unexpected disconnect",
    "transfer closed",
    "error: 502",
    "error: 503",
    "error: 504",
    " 502 ",
    " 503 ",
    " 504 ",
    "bad gateway",
    "service unavailable",
    "gateway timeout",
    "http/2 stream",
    "curl 18",
    "curl 28",
    "curl 56",
    "curl 92",
];

/// Classifies a failed Git network command by its stderr. Auth and
/// rejection patterns win over transient ones: "unable to access … 403" is an
/// auth problem, and a certificate error is never worth retrying.
pub fn classify_network_failure(stderr: &str) -> NetworkFailure {
    let text = stderr.to_ascii_lowercase();
    if AUTH_PATTERNS.iter().any(|pattern| text.contains(pattern)) {
        return NetworkFailure::Auth;
    }
    if REJECTED_PATTERNS
        .iter()
        .any(|pattern| text.contains(pattern))
    {
        return NetworkFailure::Rejected;
    }
    if PERMANENT_PATTERNS
        .iter()
        .any(|pattern| text.contains(pattern))
    {
        return NetworkFailure::Other;
    }
    if TRANSIENT_PATTERNS
        .iter()
        .any(|pattern| text.contains(pattern))
    {
        return NetworkFailure::Transient;
    }
    NetworkFailure::Other
}

/// Whether a failed network command is worth retrying.
pub fn is_transient_network_error(stderr: &str) -> bool {
    classify_network_failure(stderr) == NetworkFailure::Transient
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_failures_are_retryable() {
        for stderr in [
            "fatal: unable to access 'https://github.com/o/r.git/': Could not resolve host: github.com",
            "ssh: connect to host github.com port 22: Connection timed out",
            "error: RPC failed; curl 56 Recv failure: Connection reset by peer\nfatal: early EOF",
            "fatal: The remote end hung up unexpectedly",
            "fatal: unable to access 'https://x/': gnutls_handshake() failed: The TLS connection was non-properly terminated.",
            "fatal: unable to access 'https://x/': The requested URL returned error: 502",
            "error: RPC failed; HTTP 503 curl 22 The requested URL returned error: 503",
            "fatal: unable to access 'https://x/': Operation timed out after 300000 milliseconds",
            "fatal: unable to access 'https://x/': OpenSSL SSL_connect: SSL_ERROR_SYSCALL in connection to github.com:443",
        ] {
            assert_eq!(
                classify_network_failure(stderr),
                NetworkFailure::Transient,
                "{stderr}"
            );
            assert!(is_transient_network_error(stderr));
        }
    }

    #[test]
    fn auth_failures_are_never_retried() {
        for stderr in [
            "remote: Invalid username or password.\nfatal: Authentication failed for 'https://github.com/o/r.git/'",
            "git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.",
            "fatal: unable to access 'https://github.com/o/r.git/': The requested URL returned error: 403",
            "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
            "remote: Repository not found.\nfatal: repository 'https://github.com/o/r.git/' not found",
        ] {
            assert_eq!(
                classify_network_failure(stderr),
                NetworkFailure::Auth,
                "{stderr}"
            );
            assert!(!is_transient_network_error(stderr));
        }
    }

    #[test]
    fn rejected_pushes_are_never_retried() {
        for stderr in [
            " ! [rejected]        main -> main (non-fast-forward)\nerror: failed to push some refs to 'origin'",
            " ! [rejected]        main -> main (fetch first)",
            "remote: error: GH006: Protected branch update failed for refs/heads/main.",
            " ! [remote rejected] main -> main (pre-receive hook declined)",
        ] {
            assert_eq!(
                classify_network_failure(stderr),
                NetworkFailure::Rejected,
                "{stderr}"
            );
        }
    }

    #[test]
    fn unrelated_failures_are_other() {
        assert_eq!(
            classify_network_failure("fatal: couldn't find remote ref feature/x"),
            NetworkFailure::Other
        );
        assert_eq!(
            classify_network_failure("error: pathspec 'x' did not match"),
            NetworkFailure::Other
        );
        assert_eq!(
            classify_network_failure(
                "fatal: unable to access 'https://h/r.git/': SSL certificate problem: unable to get local issuer certificate"
            ),
            NetworkFailure::Other
        );
        assert_eq!(
            classify_network_failure(
                "fatal: unable to access 'https://h/r.git/': The requested URL returned error: 404"
            ),
            NetworkFailure::Other
        );
    }
}

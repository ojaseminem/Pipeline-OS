# Future Scope

Pipeline OS stays local-first and account-free. These are intentionally **not**
planned, because they would need accounts, servers, or code execution we can't
review:

- Team accounts, cloud sync, hosted dashboards, roles and permissions.
- Build and render farms or any remote execution service.
- License-server management.
- Automatic marketplace installs or executable plugins (extension stays declarative:
  app manifests and argument-vector tasks).
- An embedded AI model. Pipeline OS works with the AI apps you already use through
  its MCP server instead.
- Mobile apps.

Integrations that remain local and opt-in (more trackers, more engines, more DCC
presets) are welcome. Adding anything from the list above requires its own threat
model and RFC.

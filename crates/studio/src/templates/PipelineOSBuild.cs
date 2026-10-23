// Pipeline OS build script (Assets/Editor/PipelineOSBuild.cs).
//
// Installed by Pipeline OS so its Unity task presets can build headlessly:
//
//   Unity -batchmode -quit -projectPath <project> -buildTarget Android \
//         -executeMethod PipelineOS.Build.Android -pipelineosDevelopment
//
// Methods: PipelineOS.Build.Windows, .MacOS, .Linux, .Android, .IOS, .WebGL,
// .Addressables. Options (command-line arguments):
//   -pipelineosDevelopment     development build with script debugging
//   -pipelineosAppBundle       Android: build an App Bundle (.aab) instead of an APK
//   -pipelineosOutput <dir>    output folder (default: Builds/<target>)
// Android signing passwords may come from the PIPELINEOS_KEYSTORE_PASS and
// PIPELINEOS_KEYALIAS_PASS environment variables (keystore and alias are taken
// from Player Settings). Scenes are the enabled ones in File > Build Settings.
// The editor exits with 0 on success and 1 (failed build) or 2 (bad setup).
//
// This file is yours: Pipeline OS never overwrites it once it exists.

#if UNITY_EDITOR
using System;
using System.IO;
using System.Linq;
using UnityEditor;
using UnityEditor.Build.Reporting;
using UnityEngine;

namespace PipelineOS
{
    public static class Build
    {
        public static void Windows()
        {
            Run(BuildTarget.StandaloneWindows64, BuildTargetGroup.Standalone, "Windows", ".exe");
        }

        public static void MacOS()
        {
            Run(BuildTarget.StandaloneOSX, BuildTargetGroup.Standalone, "macOS", ".app");
        }

        public static void Linux()
        {
            Run(BuildTarget.StandaloneLinux64, BuildTargetGroup.Standalone, "Linux", ".x86_64");
        }

        public static void Android()
        {
            bool appBundle = HasArg("-pipelineosAppBundle");
            EditorUserBuildSettings.buildAppBundle = appBundle;
            string keystorePass = Environment.GetEnvironmentVariable("PIPELINEOS_KEYSTORE_PASS");
            string aliasPass = Environment.GetEnvironmentVariable("PIPELINEOS_KEYALIAS_PASS");
            if (!string.IsNullOrEmpty(keystorePass))
            {
                PlayerSettings.Android.keystorePass = keystorePass;
            }
            if (!string.IsNullOrEmpty(aliasPass))
            {
                PlayerSettings.Android.keyaliasPass = aliasPass;
            }
            Run(BuildTarget.Android, BuildTargetGroup.Android, "Android", appBundle ? ".aab" : ".apk");
        }

        /// Exports an Xcode project; build and sign it in Xcode on a Mac.
        public static void IOS()
        {
            Run(BuildTarget.iOS, BuildTargetGroup.iOS, "iOS", "");
        }

        public static void WebGL()
        {
            Run(BuildTarget.WebGL, BuildTargetGroup.WebGL, "WebGL", "");
        }

        /// Builds Addressables content for the active build target. Uses
        /// reflection so this file compiles without the Addressables package.
        public static void Addressables()
        {
            Type settings = Type.GetType(
                "UnityEditor.AddressableAssets.Settings.AddressableAssetSettings, Unity.Addressables.Editor");
            var build = settings?.GetMethod("BuildPlayerContent", Type.EmptyTypes);
            if (build == null)
            {
                Debug.LogError("[PipelineOS] The Addressables package (com.unity.addressables) is not installed.");
                Exit(2);
                return;
            }
            try
            {
                build.Invoke(null, null);
                Debug.Log("[PipelineOS] Addressables content build finished.");
                Exit(0);
            }
            catch (Exception error)
            {
                Debug.LogError("[PipelineOS] Addressables content build failed: " + error);
                Exit(1);
            }
        }

        static void Run(BuildTarget target, BuildTargetGroup group, string folder, string extension)
        {
            string[] scenes = EditorBuildSettings.scenes
                .Where(scene => scene.enabled && !string.IsNullOrEmpty(scene.path))
                .Select(scene => scene.path)
                .ToArray();
            if (scenes.Length == 0)
            {
                Debug.LogError("[PipelineOS] No enabled scenes in File > Build Settings.");
                Exit(2);
                return;
            }

            bool development = HasArg("-pipelineosDevelopment");
            string output = ArgValue("-pipelineosOutput") ?? Path.Combine("Builds", folder);
            string location = extension.Length == 0
                ? output
                : Path.Combine(output, SafeName(PlayerSettings.productName) + extension);
            Directory.CreateDirectory(output);

            if (EditorUserBuildSettings.activeBuildTarget != target)
            {
                EditorUserBuildSettings.SwitchActiveBuildTarget(group, target);
            }

            var options = new BuildPlayerOptions
            {
                scenes = scenes,
                locationPathName = location,
                target = target,
                targetGroup = group,
                options = development
                    ? BuildOptions.Development | BuildOptions.AllowDebugging
                    : BuildOptions.None,
            };
            Debug.Log("[PipelineOS] Building " + target + (development ? " (development)" : "")
                + " with " + scenes.Length + " scene(s) to " + location);
            BuildReport report = BuildPipeline.BuildPlayer(options);
            BuildSummary summary = report.summary;
            Debug.Log("[PipelineOS] Build " + summary.result + ": " + summary.totalErrors + " error(s), "
                + summary.totalWarnings + " warning(s), " + summary.totalSize + " bytes in "
                + summary.totalTime + " -> " + location);
            Exit(summary.result == BuildResult.Succeeded ? 0 : 1);
        }

        static bool HasArg(string name)
        {
            return Environment.GetCommandLineArgs()
                .Any(arg => string.Equals(arg, name, StringComparison.OrdinalIgnoreCase));
        }

        static string ArgValue(string name)
        {
            string[] args = Environment.GetCommandLineArgs();
            for (int i = 0; i < args.Length - 1; i++)
            {
                if (string.Equals(args[i], name, StringComparison.OrdinalIgnoreCase))
                {
                    return args[i + 1];
                }
            }
            return null;
        }

        static string SafeName(string name)
        {
            foreach (char c in Path.GetInvalidFileNameChars())
            {
                name = name.Replace(c, '_');
            }
            return string.IsNullOrWhiteSpace(name) ? "Game" : name;
        }

        static void Exit(int code)
        {
            if (Application.isBatchMode)
            {
                EditorApplication.Exit(code);
            }
        }
    }
}
#endif

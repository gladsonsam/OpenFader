import java.io.File

plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

val ciDevelopmentKeystore = providers.environmentVariable("OPENFADER_ANDROID_DEVELOPMENT_KEYSTORE").orNull
if (System.getenv("GITHUB_ACTIONS") == "true" && ciDevelopmentKeystore.isNullOrBlank()) {
    error("Android CI requires the persistent development signer; run the signer restore step first.")
}

android {
    namespace = "com.openfader.app"
    // permission_handler_android 13.x metadata requires API 37.
    compileSdk = 37
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        // TODO: Specify your own unique Application ID (https://developer.android.com/studio/build/application-id.html).
        applicationId = "com.openfader.app"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        // Uses the version code from pubspec.yaml. When using split APKs, 1000 * ABI_VERSION
        // is added automatically by Flutter. (https://developer.android.com/studio/build/configure-apk-splits#configure-APK-versions)
        // You can force using the value of versionCode by specifying the `-P force-version-code-ignoring-abi=true`
        // flag during build.
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    signingConfigs {
        if (!ciDevelopmentKeystore.isNullOrBlank()) {
            create("ciDevelopment") {
                storeFile = file(ciDevelopmentKeystore)
                storePassword = "android"
                keyAlias = "androiddebugkey"
                keyPassword = "android"
            }
        }
    }

    buildTypes {
        release {
            // CI retains its dedicated development certificate across runners.
            // Local builds keep the developer's existing debug signer.
            signingConfig = signingConfigs.getByName(
                if (ciDevelopmentKeystore.isNullOrBlank()) "debug" else "ciDevelopment")
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}

// A normal `flutter build apk` must include the engine too. Previously only
// the packaging script built it, so local APKs silently used FakeSimEngine.
val bridgeAbis = (providers.gradleProperty("target-platform").orNull
    ?: "android-arm,android-arm64,android-x64").split(",").map {
    when (it) {
        "android-arm" -> "armeabi-v7a"
        "android-arm64" -> "arm64-v8a"
        "android-x64" -> "x86_64"
        else -> error("Unsupported Rust bridge target: $it")
    }
}
// Dependencies and previous builds can supply libraries for other ABIs.
// A targeted APK must package the same ABIs for Flutter, plugins, and Rust.
for (abi in listOf("armeabi-v7a", "arm64-v8a", "x86_64")) {
    if (abi !in bridgeAbis) android.packaging.jniLibs.excludes.add("lib/$abi/**")
}
val repoRoot = rootProject.projectDir.resolve("../..").canonicalFile
val bridgeOutput = layout.buildDirectory.dir("rustJniLibs")
val bridgeNdk = androidComponents.sdkComponents.ndkDirectory
android.sourceSets.getByName("main").jniLibs.setSrcDirs(listOf(bridgeOutput))
val buildRustBridge = tasks.register<Exec>("buildRustBridge") {
    workingDir(repoRoot)
    inputs.files(fileTree(repoRoot.resolve("crates")) {
        include("**/*.rs", "**/Cargo.toml", "**/build.rs")
    }, repoRoot.resolve("Cargo.toml"), repoRoot.resolve("Cargo.lock"))
    inputs.property("abis", bridgeAbis)
    inputs.property("ndkVersion", android.ndkVersion)
    outputs.files(bridgeAbis.map {
        bridgeOutput.map { dir -> dir.file("$it/libopenfader_flutter_bridge.so") }
    })
    doFirst {
        environment("ANDROID_NDK_HOME", bridgeNdk.get().asFile)
        val cargo = File(System.getProperty("user.home"),
            ".cargo/bin/" + if (System.getProperty("os.name").startsWith("Windows")) "cargo.exe" else "cargo")
        commandLine(listOf(if (cargo.isFile) cargo.absolutePath else "cargo", "ndk") +
            bridgeAbis.flatMap { listOf("-t", it) } +
            listOf("-o", bridgeOutput.get().asFile.absolutePath, "build", "--locked",
                "--release", "-p", "openfader-flutter-bridge"))
    }
}
tasks.named("preBuild") { dependsOn(buildRustBridge) }

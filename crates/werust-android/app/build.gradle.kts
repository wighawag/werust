import org.gradle.internal.os.OperatingSystem
import org.gradle.process.ExecOperations
import javax.inject.Inject

plugins {
    id("com.android.application")
    kotlin("android")
}

android {
    namespace = "com.github.wighawag.werust"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.github.wighawag.werust"
        minSdk = 21
        targetSdk = 34
        versionCode = 1
        versionName = "0.0.0"
        // The floor ABIs: arm64-v8a for real devices, x86_64 for the emulator.
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }

        // The on-device instrumentation probe (`src/androidTest`): the strongest
        // automatable harness for the real System WebView's behaviour (task
        // `mobile-ronan-eth-buttons-no-navigation`).
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    // ---- Release signing (task `android-apk-signing`) --------------------
    //
    // The keystore and its credentials are supplied ONLY through the
    // environment, by CI, from four repository secrets (`ANDROID_KEYSTORE_B64`
    // decoded to a file that `ANDROID_KEYSTORE_PATH` points at, plus
    // `ANDROID_KEYSTORE_PASSWORD` / `ANDROID_KEY_ALIAS` / `ANDROID_KEY_PASSWORD`);
    // see the `android-apk` job in `.github/workflows/release.yml`. NOTHING about
    // the signing identity is committed — a release key cannot be regenerated,
    // so it lives outside the repo.
    //
    // Gated on env PRESENCE, which is what keeps LOCAL dev builds untouched: with
    // no `ANDROID_KEYSTORE_PATH` the `release` signing config is never created,
    // and the `release` build type below then gets a null `signingConfig` — AGP
    // leaves the build unsigned and names its output `app-release-unsigned.apk`,
    // so an unsigned build can never masquerade as the signed `app-release.apk`.
    signingConfigs {
        val keystorePath: String? = System.getenv("ANDROID_KEYSTORE_PATH")
        if (keystorePath != null) {
            create("release") {
                storeFile = file(keystorePath)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        // The debug APK: AGP's auto-generated debug keystore, i.e. installable
        // but carrying no release identity (what this repo calls the "unsigned
        // debug APK" — it is not signed with the project's key).
        getByName("debug") {
            isMinifyEnabled = false
        }
        // The release APK: the artifact a tagged release ships. Signing is the
        // ONLY thing the release build type adds — no minification/R8, so the
        // signed APK is the same code the debug APK carries (one fewer variable
        // between what CI tests and what users install).
        getByName("release") {
            isMinifyEnabled = false
            // `null` when the signing env is absent (see above): a graceful
            // no-op, never an error.
            signingConfig = signingConfigs.findByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }

    // The cross-compiled Rust core `.so`s are staged here by `cargoBuildRustCore`.
    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("rustJniLibs"))
}

dependencies {
    // The ONLY androidx dependency, and a deliberate one (task
    // `android-hardware-back-button-navigates-history`): `ComponentActivity`
    // brings the non-deprecated `OnBackPressedDispatcher`, which is how the
    // SYSTEM/hardware Back button is handled so it navigates page history instead
    // of exiting the app. The framework `android.app.Activity` offers only the
    // DEPRECATED `onBackPressed()` override, and the platform
    // `OnBackInvokedDispatcher` exists only on Android 13+ (this app's minSdk is
    // 21), so the dispatcher is the one implementation that works across
    // versions. It also bridges to the platform Android 13+ API if/when
    // predictive back is opted into. See
    // docs/spikes/android-hardware-back-button-navigates-history/README.md.
    //
    // Everything else stays framework-only: the OS edge still uses the plain
    // platform `WebView` + widgets + framework themes, so the Rust core remains
    // the only linked "library" that matters.
    implementation("androidx.activity:activity:1.9.3")

    // ON-DEVICE TEST ONLY (never shipped in the app APK): the JUnit4 runner +
    // rule support for the `androidTest` System-WebView probe. The app itself
    // stays framework-only (the single androidx dependency above).
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
}

// ---------------------------------------------------------------------------
// Cross-compile the werust Rust core as a normal Gradle build step.
//
// This is the load-bearing piece of the Zig-less build experiment (ADR-0002):
// a `cargo build` per Android ABI, linked with the NDK's clang, producing
// `libwerust_mobile.so` for arm64-v8a + x86_64, staged into `jniLibs` so the
// packaged APK carries the Rust core for both floor ABIs. The task is wired as a
// dependency of `preBuild`, so a plain `./gradlew assembleDebug` cross-compiles
// the core and packages it without any manual step.
// ---------------------------------------------------------------------------

/** The Rust workspace root: `crates/werust-android` -> `crates` -> repo root. */
val rustCoreCrate = "werust-android-core"
val workspaceRoot: File = rootProject.projectDir.parentFile.parentFile

/** Android ABI -> (Rust target triple, jniLibs dir name). */
val targetAbis = mapOf(
    "arm64-v8a" to "aarch64-linux-android",
    "x86_64" to "x86_64-linux-android",
)

/** The NDK API level the clang linker wrappers target (matches minSdk floor). */
val targetNdkApiLevel = 21

fun ndkDir(): File {
    val sdk = System.getenv("ANDROID_HOME") ?: System.getenv("ANDROID_SDK_ROOT")
    ?: throw GradleException("ANDROID_HOME / ANDROID_SDK_ROOT is not set")
    val explicit = System.getenv("ANDROID_NDK_HOME")
    if (explicit != null && File(explicit).isDirectory) return File(explicit)
    val ndkParent = File(sdk, "ndk")
    val newest = ndkParent.listFiles()?.filter { it.isDirectory }?.maxByOrNull { it.name }
        ?: throw GradleException("no NDK found under $ndkParent")
    return newest
}

fun ndkBinDir(): File {
    val hostTag = when {
        OperatingSystem.current().isMacOsX -> "darwin-x86_64"
        OperatingSystem.current().isWindows -> "windows-x86_64"
        else -> "linux-x86_64"
    }
    return File(ndkDir(), "toolchains/llvm/prebuilt/$hostTag/bin")
}

/**
 * The cross-compile task. Uses an injected [ExecOperations] (the Gradle 9 way to
 * run external processes from a task action) to run `cargo build` per ABI with
 * the NDK clang linker, then stages each `libwerust_mobile.so` into jniLibs.
 */
abstract class CargoBuildRustCore @Inject constructor(private val exec: ExecOperations) : DefaultTask() {
    @get:Internal abstract val repoRoot: DirectoryProperty
    @get:Internal abstract val ndkBin: DirectoryProperty
    @get:Input abstract val crate: Property<String>
    @get:Input abstract val ndkApiLevel: Property<Int>
    @get:Input abstract val abis: MapProperty<String, String>
    @get:OutputDirectory abstract val outDir: DirectoryProperty

    @TaskAction
    fun run() {
        val bin = ndkBin.get().asFile
        val root = repoRoot.get().asFile
        val api = ndkApiLevel.get()
        val libName = "libwerust_mobile.so"
        abis.get().forEach { (abi, triple) ->
            val cc = File(bin, "$triple$api-clang")
            val ar = File(bin, "llvm-ar")
            val linkerEnv = "CARGO_TARGET_${triple.uppercase().replace('-', '_')}_LINKER"

            exec.exec {
                workingDir = root
                environment("CC_$triple", cc.absolutePath)
                environment("AR_$triple", ar.absolutePath)
                environment(linkerEnv, cc.absolutePath)
                commandLine(
                    "cargo", "build",
                    "--release",
                    "-p", crate.get(),
                    "--target", triple,
                )
            }

            val built = File(root, "target/$triple/release/$libName")
            if (!built.isFile) {
                throw GradleException("expected $built after cargo build for $abi")
            }
            val dest = outDir.get().dir(abi).asFile
            dest.mkdirs()
            built.copyTo(File(dest, libName), overwrite = true)
        }
    }
}

val cargoBuildRustCore by tasks.registering(CargoBuildRustCore::class) {
    group = "rust"
    description = "Cross-compile the werust Rust core (libwerust_mobile.so) for the floor ABIs."
    repoRoot.set(workspaceRoot)
    ndkBin.set(ndkBinDir())
    crate.set(rustCoreCrate)
    ndkApiLevel.set(targetNdkApiLevel)
    abis.set(targetAbis)
    outDir.set(layout.buildDirectory.dir("rustJniLibs"))
}

tasks.named("preBuild") {
    dependsOn(cargoBuildRustCore)
}

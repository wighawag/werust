import org.gradle.internal.os.OperatingSystem
import org.gradle.process.ExecOperations
import javax.inject.Inject

plugins {
    id("com.android.application")
    kotlin("android")
}

// ---------------------------------------------------------------------------
// The APK's VERSION, from the release tag (task
// `android-apk-version-from-the-release-tag`).
//
// Android sequences updates on a strictly increasing INTEGER `versionCode`, so
// while this module hardcoded `versionCode = 1` / `versionName = "0.0.0"` every
// signed release looked like the SAME build to a device and could never be
// offered as an in-place update — the other half of what release signing was for.
// The user-visible `versionName` also disagreed with the version the ⋮ menu
// reports from the Rust core, which is the two-version-sources drift this repo
// keeps removing.
//
// So the version is READ from the one source that already exists — the same one
// `crates/werust-core/build.rs` resolves — rather than minted a second time:
//
//   1. `WERUST_VERSION`, which the `android-apk` job in
//      `.github/workflows/release.yml` exports from the tag ref name (the same
//      variable the cross-compiled Rust core the APK carries is built with — and
//      whatever resolves here is handed BACK to that cross-compile below, so the
//      manifest and the ⋮ menu cannot disagree even on an incremental build).
//   2. else `git describe --tags --always` (an informative local dev build).
//   3. else the workspace Cargo version, the last resort (no git, a source
//      tarball) — the same last resort, read from the same `Cargo.toml`
//      `build.rs` falls back to.
//
// Every lookup is failure-TOLERANT: a local untagged build must still succeed on
// a placeholder, because a dev APK with a placeholder version is a far better
// outcome than a dev build that fails. That tolerance stops at the RELEASE path:
// when CI injected `WERUST_VERSION` from a tag and it folds to no `versionCode`
// (`v0.3.0-rc1`), the build FAILS instead, because a placeholder on a signed
// release APK is the unsequenceable artifact this whole block exists to prevent
// (task `android-release-tag-that-is-not-a-triple-must-not-ship-versioncode-1`).
//
// Decisions (the `major * 10000 + minor * 100 + patch` fold, the rejected CI
// run number, and why the resolution is mirrored here instead of shelling out to
// the Rust core): docs/spikes/android-apk-signing/README.md.
// Pinned by `crates/werust-core/tests/release_plumbing_shape.rs` (criterion 10).
// ---------------------------------------------------------------------------

/** The Rust workspace root: `crates/werust-android` -> `crates` -> repo root. */
val workspaceRoot: File = rootProject.projectDir.parentFile.parentFile

/**
 * The version an untagged local build carries: exactly what this module
 * hardcoded before, kept as the FALLBACK so `./gradlew :app:assembleDebug` on a
 * checkout with no tag (or no git at all) builds and installs as it always did.
 */
val devPlaceholderVersionCode = 1

/** The `versionName` of last resort, when no version source resolves at all. */
val devPlaceholderVersionName = "0.0.0"

/**
 * The version CI INJECTED from the release tag, trimmed, or `null` when
 * `WERUST_VERSION` is unset or blank. This is the DEV-versus-RELEASE
 * distinction, and everything tolerant below is keyed on it.
 *
 * It is deliberately NOT the shape of the resolved string: the other two sources
 * (`git describe`, the workspace Cargo version) are dev sources by construction,
 * so only the PRESENCE of this variable says "CI is cutting a release from a
 * tag" — the one condition under which a placeholder version would be shipped to
 * users rather than sitting on a developer's device (task
 * `android-release-tag-that-is-not-a-triple-must-not-ship-versioncode-1`).
 */
val injectedReleaseVersion: String? = System.getenv("WERUST_VERSION")?.trim()?.takeIf { it.isNotEmpty() }

/**
 * `git describe --tags --always` at the workspace root, or `null` when git is
 * absent, the tree is not a checkout, or the command fails for any other reason.
 * The Kotlin twin of `build.rs`'s `git_describe`, and deliberately as
 * failure-tolerant: a missing version source degrades to the next one, never
 * breaks a build.
 */
fun gitDescribe(): String? = try {
    val process = ProcessBuilder("git", "describe", "--tags", "--always")
        .directory(workspaceRoot)
        .redirectError(ProcessBuilder.Redirect.DISCARD)
        .start()
    val described = process.inputStream.bufferedReader().use { it.readText() }.trim()
    if (process.waitFor() == 0 && described.isNotEmpty()) described else null
} catch (e: Exception) {
    null
}

/** A `version = "x.y.z"` line of a Cargo manifest's `[workspace.package]` table. */
val cargoVersionLine = Regex("^version\\s*=\\s*\"([^\"]+)\"")

/**
 * The workspace Cargo version (`[workspace.package] version`), the LAST-RESORT
 * source `build.rs` reads as `CARGO_PKG_VERSION`. Read out of the SAME
 * `Cargo.toml` rather than duplicated here, and `null` on any surprise.
 */
fun cargoWorkspaceVersion(): String? = try {
    val lines = File(workspaceRoot, "Cargo.toml").readLines()
    val start = lines.indexOfFirst { it.trim() == "[workspace.package]" }
    if (start < 0) {
        null
    } else {
        lines.drop(start + 1)
            .takeWhile { !it.trim().startsWith("[") }
            .firstNotNullOfOrNull { cargoVersionLine.find(it.trim()) }
            ?.groupValues?.get(1)
    }
} catch (e: Exception) {
    null
}

/**
 * Strip a release tag's leading `v` when it prefixes a version NUMBER
 * (`v0.2.6` -> `0.2.6`), leaving anything else untouched — the same
 * normalisation `build.rs` applies, so the two never render the version
 * differently.
 */
fun stripTagPrefix(version: String): String =
    if (version.length > 1 && version[0] == 'v' && version[1].isDigit()) version.substring(1) else version

/**
 * werust's ONE version string, resolved from the sources in precedence order, or
 * `null` when NO source resolves at all (no injection, no git, no readable
 * `Cargo.toml`) — the caller then takes the placeholder, and nothing is injected
 * into the Rust build, so `build.rs` keeps its own last resort rather than being
 * told a version this script guessed.
 *
 * Note for a LOCAL build: `System.getenv` here is the Gradle DAEMON's
 * environment, so a daemon reused across shells can carry the `WERUST_VERSION`
 * of an earlier invocation (`./gradlew --stop` clears it). CI starts a fresh
 * daemon per job, so the tagged path is unaffected — and whatever this resolves
 * is used for BOTH the manifest and the compiled core (below), so the two can
 * never disagree even when it is stale.
 */
fun resolveWerustVersion(): String? {
    val resolved = listOf(injectedReleaseVersion, gitDescribe(), cargoWorkspaceVersion())
        .firstOrNull { !it.isNullOrBlank() }
        ?.trim()
        ?: return null
    return stripTagPrefix(resolved)
}

/**
 * Fold a released semver triple into ONE monotonic integer `versionCode`:
 * `major * 10000 + minor * 100 + patch` (`0.2.9` -> 209, `0.3.0` -> 300,
 * `1.0.0` -> 10000). Monotonic across every release this project will plausibly
 * cut, and readable back by eye.
 *
 * `null` for anything that is not a CLEAN triple — a `git describe` suffix
 * (`0.2.9-13-gabc1234`), a pre-release tag, an operator's named build — because
 * those are not released versions and must take the dev placeholder instead of
 * folding into a meaningless code.
 *
 * A triple whose minor or patch exceeds 99 would COLLIDE with the next
 * minor/major (`0.100.0` and `1.0.0` both fold to 10000), silently breaking the
 * update sequencing this mapping exists to provide, so it fails the build loudly
 * instead. See decision 5 in docs/spikes/android-apk-signing/README.md.
 */
fun versionCodeOf(version: String): Int? {
    val triple = Regex("""^(\d+)\.(\d+)\.(\d+)$""").matchEntire(version) ?: return null
    val (major, minor, patch) = triple.destructured.toList().map(String::toInt)
    if (minor > 99 || patch > 99) {
        throw GradleException(
            "version $version cannot be folded into a monotonic versionCode: minor and patch " +
                "must each be <= 99 for `major * 10000 + minor * 100 + patch` not to collide " +
                "with the next minor/major. Widen the mapping (and never lower a released " +
                "versionCode) — see docs/spikes/android-apk-signing/README.md.",
        )
    }
    return major * 10000 + minor * 100 + patch
}

/**
 * The resolved version, or `null` when no source has one. It is handed to BOTH
 * the APK manifest (below) and the cargo cross-compile (`cargoBuildRustCore`),
 * which is what keeps the manifest and the ⋮ menu from disagreeing: without it
 * the cross-compile is an UP-TO-DATE-checked task, so a local rebuild after the
 * version changed would re-stamp the manifest while repackaging a `.so` compiled
 * with the PREVIOUS version.
 */
val resolvedWerustVersion: String? = resolveWerustVersion()

/** The version string shown to users — the same one the ⋮ menu reports. */
val werustVersionName: String = resolvedWerustVersion ?: devPlaceholderVersionName

/**
 * The integer Android sequences updates on. A folded release triple, else — on a
 * DEV build only — the placeholder, which also covers the `0.0.0` placeholder
 * name (no source resolved at all), so the manifest never carries
 * `versionCode = 0`.
 *
 * The tolerance is keyed on dev-versus-release, NOT on the shape of the string.
 * `.github/workflows/release.yml` triggers on `tags: [v*]`, so `v0.3.0-rc1` is
 * an acceptable release tag: it resolves a correct `versionName` but folds to no
 * `versionCode`, and a placeholder there would attach a SIGNED release APK
 * carrying `versionCode = 1` — unsequenceable, un-updatable, indistinguishable
 * from every dev build, i.e. exactly the bug the version mapping exists to
 * remove. So an INJECTED version that cannot be folded fails the build loudly,
 * the same treatment decision 5 gives a component that would collide, while a
 * build with no injected version keeps today's tolerant placeholder so a local
 * `./gradlew :app:assembleDebug` still builds and installs.
 *
 * Sequencing pre-release tags instead of rejecting them is a product decision
 * this deliberately does NOT make; see decision 8 in
 * docs/spikes/android-apk-signing/README.md.
 */
val werustVersionCode: Int = versionCodeOf(werustVersionName)?.takeIf { it > 0 } ?: run {
    if (injectedReleaseVersion != null) {
        throw GradleException(
            "WERUST_VERSION=$injectedReleaseVersion cannot be folded into a versionCode, so this " +
                "release APK could not be sequenced as an update: Android orders updates by a " +
                "strictly increasing INTEGER versionCode, and only a clean major.minor.patch " +
                "triple (optionally `v`-prefixed, minor and patch each <= 99) folds into one via " +
                "`major * 10000 + minor * 100 + patch`. Falling back to the dev placeholder " +
                "($devPlaceholderVersionCode) would ship a signed release no device could ever " +
                "offer as an update. Tag a clean triple (e.g. `v0.3.0`); sequencing a " +
                "pre-release tag such as `v0.3.0-rc1` is deliberately NOT designed — see " +
                "decision 8 in docs/spikes/android-apk-signing/README.md.",
        )
    }
    devPlaceholderVersionCode
}

android {
    namespace = "com.github.wighawag.werust"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.github.wighawag.werust"
        minSdk = 21
        targetSdk = 34
        // From the release tag, via the ONE version source (see the block above).
        versionCode = werustVersionCode
        versionName = werustVersionName
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

/** The Rust crate the app links (`workspaceRoot` is declared with the version
 * resolution above, because the `android { }` block already needs it). */
val rustCoreCrate = "werust-android-core"

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

    /**
     * The resolved werust version, exported to cargo as `WERUST_VERSION` so
     * `crates/werust-core/build.rs` compiles the SAME string the APK manifest
     * declares. Declared as an `@Input` (empty when no version resolved) so a
     * changed version actually RE-RUNS this task instead of repackaging a
     * stale core.
     */
    @get:Input abstract val werustVersion: Property<String>
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
                // Only when a version actually resolved: injecting an empty (or
                // guessed) value would override build.rs's own resolution chain.
                werustVersion.get().takeIf { it.isNotEmpty() }?.let {
                    environment("WERUST_VERSION", it)
                }
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
    werustVersion.set(resolvedWerustVersion ?: "")
    ndkApiLevel.set(targetNdkApiLevel)
    abis.set(targetAbis)
    outDir.set(layout.buildDirectory.dir("rustJniLibs"))
}

tasks.named("preBuild") {
    dependsOn(cargoBuildRustCore)
}

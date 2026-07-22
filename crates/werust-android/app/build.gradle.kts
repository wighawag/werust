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
    }

    // Unsigned debug APK only — signing/store is out of scope for this task.
    buildTypes {
        getByName("debug") {
            isMinifyEnabled = false
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
    // No androidx dependency: the OS edge uses a plain framework `Activity`,
    // `WebView`, and widgets, so the Rust core is the only linked "library" that
    // matters and the app builds with just the platform SDK.
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

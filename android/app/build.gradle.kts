import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// ---------------------------------------------------------------------------
// Signature de release
//
// La clé n'est jamais versionnée. En local elle est décrite par
// `android/keystore.properties` ; en CI par des variables d'environnement
// alimentées depuis les secrets GitHub. Sans clé, on retombe sur la signature
// debug pour que `assembleRelease` reste utilisable par n'importe qui.
// ---------------------------------------------------------------------------

val keystoreProperties = Properties().apply {
    val file = rootProject.file("keystore.properties")
    if (file.exists()) file.inputStream().use { load(it) }
}

fun signingValue(key: String, env: String): String? =
    keystoreProperties.getProperty(key) ?: System.getenv(env)

// Chemin relatif : compris depuis `android/`, pas depuis le module `app`.
val releaseStoreFile: File? = signingValue("storeFile", "TIMEWRAP_STORE_FILE")
    ?.let { path -> File(path).takeIf { it.isAbsolute } ?: rootProject.file(path) }
val hasReleaseKey: Boolean = releaseStoreFile?.isFile == true

// ---------------------------------------------------------------------------
// Chaîne Rust : cargo-ndk compile le cœur pour chaque ABI, puis uniffi-bindgen
// dérive les bindings Kotlin depuis la bibliothèque produite.
// ---------------------------------------------------------------------------

val rustDir: File = rootProject.file("../core")
val rustAbis = listOf("arm64-v8a", "armeabi-v7a", "x86_64")
val jniLibsDir: File = layout.buildDirectory.dir("rustJniLibs").get().asFile
val uniffiDir: File = layout.buildDirectory.dir("generated/uniffi").get().asFile
val isWindows = System.getProperty("os.name").startsWith("Windows", ignoreCase = true)
val cargo = if (isWindows) "cargo.exe" else "cargo"

android {
    namespace = "app.timewrap"
    compileSdk = 36

    defaultConfig {
        applicationId = "app.timewrap"
        minSdk = 29
        targetSdk = 36
        versionCode = 4
        versionName = "0.4.0"
        ndk { abiFilters += rustAbis }
    }

    signingConfigs {
        if (hasReleaseKey) {
            create("release") {
                storeFile = releaseStoreFile
                storePassword = signingValue("storePassword", "TIMEWRAP_STORE_PASSWORD")
                keyAlias = signingValue("keyAlias", "TIMEWRAP_KEY_ALIAS")
                keyPassword = signingValue("keyPassword", "TIMEWRAP_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = if (hasReleaseKey) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
        }
        debug {
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
    }

    sourceSets["main"].jniLibs.srcDir(jniLibsDir)
    sourceSets["main"].kotlin.srcDir(uniffiDir)

    packaging {
        resources.excludes += setOf("/META-INF/{AL2.0,LGPL2.1}")
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

/** NDK à utiliser : variable d'environnement en CI, sinon le plus récent du SDK local. */
fun resolveNdkHome(): String {
    sequenceOf("ANDROID_NDK_HOME", "ANDROID_NDK_ROOT", "ANDROID_NDK_LATEST_HOME")
        .mapNotNull { System.getenv(it) }
        .firstOrNull { it.isNotBlank() && File(it).isDirectory }
        ?.let { return it }

    val ndkRoot = File(android.sdkDirectory, "ndk")
    val newest = ndkRoot.listFiles()?.filter { it.isDirectory }?.maxByOrNull { it.name }
        ?: error("Aucun NDK trouvé dans $ndkRoot — installe-le via le SDK Manager d'Android Studio.")
    return newest.absolutePath
}

val cargoBuildRust by tasks.registering(Exec::class) {
    group = "rust"
    description = "Compile le cœur Rust pour les ABI Android (cargo-ndk)."
    workingDir = rustDir

    val command = mutableListOf(cargo, "ndk")
    rustAbis.forEach { command += listOf("-t", it) }
    command += listOf("-o", jniLibsDir.absolutePath, "build", "--release")
    commandLine(command)

    inputs.dir(rustDir.resolve("src"))
    inputs.file(rustDir.resolve("Cargo.toml"))
    outputs.dir(jniLibsDir)

    doFirst {
        environment("ANDROID_NDK_HOME", resolveNdkHome())
    }
}

val generateUniffiBindings by tasks.registering(Exec::class) {
    group = "rust"
    description = "Génère les bindings Kotlin depuis la bibliothèque native (UniFFI)."
    dependsOn(cargoBuildRust)
    workingDir = rustDir

    commandLine(
        cargo, "run", "--quiet", "--bin", "uniffi-bindgen", "--",
        "generate",
        "--library", File(jniLibsDir, "arm64-v8a/libtimewrap_core.so").absolutePath,
        "--language", "kotlin",
        "--no-format",
        "--out-dir", uniffiDir.absolutePath,
    )

    inputs.dir(jniLibsDir)
    outputs.dir(uniffiDir)
}

tasks.named("preBuild") {
    dependsOn(generateUniffiBindings)
}

// Les `.so` sont une sortie de tâche servie comme dossier de sources : sans ce
// lien explicite, Gradle refuse la dépendance implicite entre les deux.
tasks.matching { it.name.startsWith("merge") && it.name.endsWith("JniLibFolders") }
    .configureEach { dependsOn(cargoBuildRust) }

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.work)

    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.material.icons)

    // Requis par les bindings UniFFI : JNA pour l'appel natif, coroutines pour
    // le support asynchrone que le générateur émet systématiquement.
    implementation(variantOf(libs.jna) { artifactType("aar") })
    implementation(libs.kotlinx.coroutines)

    debugImplementation(libs.androidx.compose.ui.tooling)
}

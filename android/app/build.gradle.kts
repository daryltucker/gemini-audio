plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// Task to generate Kotlin bindings from Rust library
// Automatically triggered by `make android-install` / `make android-build`
tasks.register<Exec>("generateKotlinBindings") {
    group = "uniffi"
    description = "Generate Kotlin bindings from Rust library using UniFFI"
    
    // Use a script file to avoid multiline issues
    workingDir = rootProject.rootDir
    commandLine("bash", "/home/daryl/Projects/NRG/gemini-audio/android/app/scripts/generate-bindings.sh")
}

val versionNameProp = (project.findProperty("versionName") as String?) ?: "0.1.0"
val versionCodeProp = (project.findProperty("versionCode") as String?)?.toIntOrNull() ?: 1

android {
    namespace = "audio.gemini.app"
    compileSdk = 35

    defaultConfig {
        applicationId = "audio.gemini.app"
        minSdk = 31
        targetSdk = 35
        versionCode = versionCodeProp
        versionName = versionNameProp

        ndk {
            abiFilters += listOf("arm64-v8a")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    applicationVariants.all {
        val variant = this
        outputs.all {
            val output = this as com.android.build.gradle.internal.api.BaseVariantOutputImpl
            if (variant.buildType.name == "release") {
                output.outputFileName = "gemini-audio-android-v${variant.versionName}.apk"
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
    }

    // Tell Gradle where to find the Rust .so files
    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }
}

dependencies {
    // Compose BOM — single version for all Compose libs
    val composeBom = platform("androidx.compose:compose-bom:2024.12.01")
    implementation(composeBom)

    // Core Compose
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")

    // Activity + Lifecycle
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.7")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")

    // Navigation
    implementation("androidx.navigation:navigation-compose:2.8.5")

    // DataStore for preferences (API key, settings)
    implementation("androidx.datastore:datastore-preferences:1.1.2")

    // Kotlin coroutines
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")

    // JNA for UniFFI
    implementation("net.java.dev.jna:jna:5.14.0@aar")

    // Oboe for low-latency audio (optional - we use native AudioRecord/AudioTrack)
    implementation("com.google.oboe:oboe:1.8.0")

    // Debug tooling
    debugImplementation("androidx.compose.ui:ui-tooling")
}

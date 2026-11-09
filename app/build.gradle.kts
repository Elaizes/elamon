import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
	id("com.android.application")
	id("org.jetbrains.kotlin.android")
	id("org.jetbrains.kotlin.plugin.compose")
	id("org.jetbrains.kotlin.plugin.serialization")
}

android {
	namespace = "ua.elamon"
	compileSdk = 35
	buildToolsVersion = "35.0.0"

	defaultConfig {
		applicationId = "ua.elamon"
		minSdk = 26
		targetSdk = 35
		versionCode = 2
		versionName = "0.2.0"
	}

	buildTypes {
		release {
			isMinifyEnabled = true
			isShrinkResources = true
			proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
		}
	}

	// F-Droid builds from source and rejects the encrypted dependency list Google Play wants.
	dependenciesInfo {
		includeInApk = false
		includeInBundle = false
	}

	buildFeatures {
		compose = true
		buildConfig = true
	}

	compileOptions {
		sourceCompatibility = JavaVersion.VERSION_17
		targetCompatibility = JavaVersion.VERSION_17
	}
}

kotlin {
	compilerOptions {
		jvmTarget.set(JvmTarget.JVM_17)
	}
}

dependencies {
	val composeBom = platform("androidx.compose:compose-bom:2024.12.01")
	implementation(composeBom)
	implementation("androidx.compose.ui:ui")
	implementation("androidx.compose.foundation:foundation")
	implementation("androidx.compose.material3:material3")
	implementation("androidx.compose.material:material-icons-core")
	implementation("androidx.activity:activity-compose:1.9.3")
	implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
	implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")
	implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
	implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
	debugImplementation("androidx.compose.ui:ui-tooling")
}

// F-Droid rebuilds the APK to compare with the published one; ART profiles differ from build to build.
tasks.whenTaskAdded {
	if (name.contains("ArtProfile")) enabled = false
}

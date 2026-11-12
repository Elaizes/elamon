import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
	id("com.android.application")
	id("org.jetbrains.kotlin.plugin.compose")
	id("org.jetbrains.kotlin.plugin.serialization")
}

/**
 * The release key, from a file outside the repository: storeFile, storePassword, keyAlias and
 * keyPassword, a relative storeFile counting from the file's folder. By default
 * ~/.android/elamon-release.properties, or wherever -Pelamon.signing=… says. Without the file the
 * release APK stays unsigned, as F-Droid builds it (it signs with its own key). The passwords
 * only go to the signing config, never to the output.
 */
val releaseSigning: Properties? = run {
	// A relative -Pelamon.signing counts from the project's root.
	val file = providers.gradleProperty("elamon.signing").orNull?.let { rootProject.file(it) }
		?: File(System.getProperty("user.home"), ".android/elamon-release.properties")
	if (!file.isFile) return@run null
	Properties().apply {
		file.inputStream().use { load(it) }
		val missing = listOf("storeFile", "storePassword", "keyAlias", "keyPassword").filter { getProperty(it).isNullOrEmpty() }
		if (missing.isNotEmpty()) throw GradleException("$file lacks ${missing.joinToString()}")
		setProperty("storeFile", file.absoluteFile.parentFile.resolve(getProperty("storeFile")).path)
	}
}

android {
	namespace = "io.github.elaizes.elamon"
	compileSdk = 37
	buildToolsVersion = "37.0.0"

	defaultConfig {
		applicationId = "io.github.elaizes.elamon"
		minSdk = 26
		targetSdk = 36
		versionCode = 6
		versionName = "0.6.0"
	}

	signingConfigs {
		if (releaseSigning != null) {
			create("release") {
				storeFile = file(releaseSigning.getProperty("storeFile"))
				storePassword = releaseSigning.getProperty("storePassword")
				keyAlias = releaseSigning.getProperty("keyAlias")
				keyPassword = releaseSigning.getProperty("keyPassword")
			}
		}
	}

	buildTypes {
		// Beside the real app, never in its place: installing or uninstalling a debug build (as
		// maestro/run.sh does) leaves the real app and its pairings alone. Its launcher label says so.
		debug {
			applicationIdSuffix = ".debug"
		}
		release {
			isMinifyEnabled = true
			isShrinkResources = true
			proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
			if (releaseSigning != null) signingConfig = signingConfigs.getByName("release")
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

	// AGP 9 checks Compose previews against compiled resources, through the unit-test setup.
	testOptions {
		unitTests.isIncludeAndroidResources = true
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
	val composeBom = platform("androidx.compose:compose-bom:2026.09.00")
	implementation(composeBom)
	implementation("androidx.compose.ui:ui")
	implementation("androidx.compose.foundation:foundation")
	implementation("androidx.compose.material3:material3")
	implementation("androidx.compose.material:material-icons-core")
	// Frosted-glass bars over scrolling content (RenderEffect on Android 12+, a tint before).
	implementation("dev.chrisbanes.haze:haze:1.7.3")
	implementation("dev.chrisbanes.haze:haze-materials:1.7.3")
	implementation("androidx.activity:activity-compose:1.13.0")
	// Unlocking the phone before an action, down to Android 8: the prompt lives in a FragmentActivity
	// and, before Android 9, shows a fingerprint dialog of AppCompat's, whose theme the app's extends.
	implementation("androidx.biometric:biometric:1.1.0")
	implementation("androidx.fragment:fragment:1.9.1")
	implementation("androidx.appcompat:appcompat:1.8.0")
	implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.11.0")
	implementation("androidx.lifecycle:lifecycle-runtime-compose:2.11.0")
	implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.12.0")
	implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.11.0")
	debugImplementation("androidx.compose.ui:ui-tooling")

	testImplementation("junit:junit:4.13.2")
	testImplementation("org.jetbrains.kotlin:kotlin-test-junit:2.4.21")
	testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.11.0")
}

// elamond's tests write a JSON file for every shape it sends; the app's decode each of them.
val protocolFixtures: Directory = rootProject.layout.projectDirectory.dir("elamond/fixtures")
tasks.withType<Test>().configureEach {
	inputs.dir(protocolFixtures).withPropertyName("protocolFixtures").withPathSensitivity(PathSensitivity.RELATIVE)
	systemProperty("elamon.fixtures", protocolFixtures.asFile.absolutePath)
}

// F-Droid rebuilds the APK to compare with the published one; ART profiles differ from build to build.
// Gradle 9 registers these lazily, past whenTaskAdded; configureEach reaches them.
tasks.configureEach {
	if (name.contains("ArtProfile")) enabled = false
}

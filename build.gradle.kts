plugins {
	// AGP 9 compiles Kotlin itself; the Kotlin version comes from the plugins below.
	id("com.android.application") version "9.5.1" apply false
	id("org.jetbrains.kotlin.plugin.compose") version "2.4.21" apply false
	id("org.jetbrains.kotlin.plugin.serialization") version "2.4.21" apply false
}

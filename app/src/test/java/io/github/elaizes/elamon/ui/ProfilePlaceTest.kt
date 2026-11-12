package io.github.elaizes.elamon.ui

import io.github.elaizes.elamon.data.Profile
import io.github.elaizes.elamon.data.ProfileSource
import kotlin.test.Test
import kotlin.test.assertEquals

/** Where Settings says a phone's profile is set on the computer. */
class ProfilePlaceTest {
	private fun at(file: String, line: Int) = Profile(id = "galaxy-s25", source = ProfileSource(file, line))

	@Test
	fun anAdminIsToldTheFileAndLine() {
		assertEquals(ProfilePlace.InFolder("elamond.d/phones.toml", 4), profilePlace(at("elamond.d/phones.toml", 4)))
		assertEquals(ProfilePlace.InFolder("elamond.toml", 61), profilePlace(at("elamond.toml", 61)))
		// Outside the settings folder: by its path.
		assertEquals(ProfilePlace.AtPath("/etc/elamon/desk.toml", 3), profilePlace(at("/etc/elamon/desk.toml", 3)))
		// Written by Nix: set there, not in the file.
		assertEquals(ProfilePlace.Nix("galaxy-s25"), profilePlace(at("/nix/store/0000000000000000000000000000000-elamond.toml", 61)))
		// Nothing in it reads as something else.
		assertEquals(ProfilePlace.InFolder("elamond.d/phones.toml", 4), profilePlace(at("elamond.d/‮phones.toml", 4)))
	}

	@Test
	fun otherRolesAreToldTheTable() {
		assertEquals(ProfilePlace.Table("galaxy-s25"), profilePlace(Profile(id = "galaxy-s25")))
		assertEquals(ProfilePlace.Table("default"), profilePlace(Profile()))
	}
}

package io.github.elaizes.elamon.data

import kotlin.test.Test
import kotlin.test.assertEquals

/** A lazy list throws on a key given twice: what the service or the network sends twice is told apart. */
class KeysTest {
	@Test
	fun eachKeyOnce() {
		val keys = UniqueKeys()
		val given = listOf("header", "row/a", "row/a", "row/a", "table/p/1", "table/p/1", "row/a#2").map(keys::of)
		assertEquals(listOf("header", "row/a", "row/a#2", "row/a#3", "table/p/1", "table/p/1#2", "row/a#2#2"), given)
		assertEquals(given.size, given.toSet().size)
	}

	@Test
	fun aTableHeadAndAnotherTablesRowDontMeet() {
		// The table "p/q" and the row "q" of the table "p": the same text, still two keys.
		val keys = UniqueKeys()
		assertEquals("table/p/q", keys.of("table/p/q"))
		assertEquals("table/p/q#2", keys.of("table/p/q"))
	}

	@Test
	fun aComputerFoundTwiceIsListedOnce() {
		val a = Found("0123456789abcdef0123456789abcdef", "deskpc", "192.0.2.10", 47823, null)
		// Another announcement with its id: the computer renamed, or something copying it.
		val copy = a.copy(name = "deskpc (2)", host = "192.0.2.66")
		val b = Found("fedcba9876543210fedcba9876543210", "laptop", "192.0.2.11", 47823, null)
		assertEquals(listOf(a, b), unpairedFound(listOf(a, copy, b), emptyList()))
		assertEquals(listOf(b), unpairedFound(listOf(a, copy, b), listOf(a.id)))
	}
}

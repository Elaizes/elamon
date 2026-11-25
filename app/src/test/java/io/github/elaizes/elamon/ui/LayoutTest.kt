package io.github.elaizes.elamon.ui

import androidx.compose.ui.unit.dp
import io.github.elaizes.elamon.data.Grid
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/** What the layout's rules come down to in numbers, apart from drawing. */
class LayoutTest {
	@Test
	fun whatFitsKeepsItsSize() {
		assertEquals(1f, shrinkTo(100, 100, 0.6f))
		assertEquals(1f, shrinkTo(100, 40, 0.6f))
		// Nothing to fit, nothing to shrink.
		assertEquals(1f, shrinkTo(0, 0, 0.6f))
	}

	@Test
	fun whatDoesntFitGetsSmallerButStaysReadable() {
		assertEquals(0.8f, shrinkTo(80, 100, 0.6f))
		assertEquals(0.6f, shrinkTo(10, 100, 0.6f))
		assertEquals(0.8f, shrinkTo(10, 100, 0.8f))
	}

	@Test
	fun aNameOfPartsBreaksBetweenThem() {
		assertEquals("qemu-\u200bsystem-\u200bx86_\u200b64", breakable("qemu-system-x86_64"))
		assertEquals("Local web", breakable("Local web"))
		assertEquals("a--\u200bb-", breakable("a--b-"))
	}

	@Test
	fun aCellsWidthIsItsShareOfTheRow() {
		val row = Grid.place(listOf(2, 1, 1), 4).single()
		val widths = row.cells.map { (_, span) -> cellWidth(row, span, 328.dp) }
		// With the gaps between them, the row.
		assertEquals(328f, widths.sumOf { it.value.toDouble() }.toFloat() + 2 * Space.gap.value, 0.01f)
		assertEquals(widths[1], widths[2])
		// Two columns and the gap between them.
		assertEquals(widths[1].value * 2 + Space.gap.value, widths[0].value, 0.01f)
		// One alone in its row has all of it.
		val alone = Grid.place(listOf(1), 4).single()
		assertEquals(328.dp, cellWidth(alone, 1, 328.dp))
	}

	@Test
	fun columnsOfWordsLeaveTheNamesTheirRoom() {
		// What fits together stays as it is.
		assertEquals(mapOf("a" to 35.dp, "b" to 32.dp), columnsWithin(mapOf("a" to 35.dp, "b" to 32.dp), 148.dp))
		// A long one gives up what there isn't; a short one keeps its own.
		assertEquals(mapOf("a" to 35.dp, "b" to 113.dp), columnsWithin(mapOf("a" to 35.dp, "b" to 300.dp), 148.dp))
		// Two long ones share it.
		assertEquals(mapOf("a" to 74.dp, "b" to 74.dp), columnsWithin(mapOf("a" to 200.dp, "b" to 300.dp), 148.dp))
		// Never to nothing.
		assertEquals(mapOf("a" to Sizes.columnLeast, "b" to Sizes.columnLeast), columnsWithin(mapOf("a" to 200.dp, "b" to 300.dp), 20.dp))
	}

	@Test
	fun spacingInStepsOfFourAndFewCorners() {
		assertTrue(listOf(Space.xs, Space.s, Space.m, Space.l, Space.xl, Space.xxl).all { it.value % 4 == 0f })
		// A corner inside another is the outer one less what is between them.
		assertEquals(Radius.card, Radius.inner + Space.card)
		assertEquals(Radius.card, Radius.tab + Space.xs)
		assertTrue(setOf(Radius.xs, Radius.s, Radius.inner, Radius.tab, Radius.card).size <= 6)
	}
}

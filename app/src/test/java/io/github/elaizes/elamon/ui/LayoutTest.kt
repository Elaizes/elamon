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
		assertEquals(1f, shrinkTo(100, 100))
		assertEquals(1f, shrinkTo(100, 40))
		// Nothing to fit, nothing to shrink.
		assertEquals(1f, shrinkTo(0, 0))
	}

	@Test
	fun whatDoesntFitGetsSmallerButStaysReadable() {
		assertEquals(0.8f, shrinkTo(80, 100))
		assertEquals(0.6f, shrinkTo(10, 100))
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
	fun spacingInStepsOfFourAndFewCorners() {
		assertTrue(listOf(Space.xs, Space.s, Space.m, Space.l, Space.xl, Space.xxl).all { it.value % 4 == 0f })
		// A corner inside another is the outer one less what is between them.
		assertEquals(Radius.card, Radius.inner + Space.card)
		assertEquals(Radius.card, Radius.tab + Space.xs)
		assertTrue(setOf(Radius.xs, Radius.s, Radius.inner, Radius.tab, Radius.card).size <= 6)
	}
}

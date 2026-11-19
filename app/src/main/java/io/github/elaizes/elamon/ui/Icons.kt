package io.github.elaizes.elamon.ui

import androidx.annotation.DrawableRes
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.vectorResource
import io.github.elaizes.elamon.R

/** Material Symbols (Outlined, Apache-2.0) as vector drawables, `res/drawable/ic_*.xml`. */
object ElIcons {
	val Monitor: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_monitor)
	val Eye: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_eye)
	val Clock: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_clock)
	val Laptop: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_laptop)
	val Thermometer: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_thermometer)
	val Chip: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_chip)
	val Memory: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_memory)
	val Gpu: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_gpu)
	val Network: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_network)
	val Storage: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_storage)
	val Battery: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_battery)
	val Fan: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_fan)
	val Bolt: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_bolt)
	val Processes: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_processes)
	val Overview: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_overview)
	val Power: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_power)
	val Restart: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_restart)
	val Moon: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_moon)
	val Lock: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_lock)
	val Play: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_play)
	val Pause: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_pause)
	val Next: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_next)
	val Previous: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_previous)
	val Volume: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_volume)
	val Mute: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_mute)
	val Terminal: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_terminal)
	val Globe: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_globe)
	val Leaf: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_leaf)
	val Gauge: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_gauge)
	val Rocket: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_rocket)
	val Music: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_music)
	val Control: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_control)
	val Copy: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_copy)
	val Help: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_help)
	val Qr: ImageVector @Composable get() = ImageVector.vectorResource(R.drawable.ic_qr)
}

/** The drawable of an icon the settings name (docs/config.md, "Icons"), as a tile draws it; `overview` for one this app doesn't know. */
@DrawableRes
fun iconRes(name: String?): Int = when (name) {
	"battery" -> R.drawable.ic_battery
	"bolt" -> R.drawable.ic_bolt
	"chip" -> R.drawable.ic_chip
	"clock" -> R.drawable.ic_clock
	"control" -> R.drawable.ic_control
	"eye" -> R.drawable.ic_eye
	"fan" -> R.drawable.ic_fan
	"gauge" -> R.drawable.ic_gauge
	"globe" -> R.drawable.ic_globe
	"gpu" -> R.drawable.ic_gpu
	"help" -> R.drawable.ic_help
	"laptop" -> R.drawable.ic_laptop
	"leaf" -> R.drawable.ic_leaf
	"lock" -> R.drawable.ic_lock
	"memory" -> R.drawable.ic_memory
	"monitor" -> R.drawable.ic_monitor
	"moon" -> R.drawable.ic_moon
	"music" -> R.drawable.ic_music
	"network" -> R.drawable.ic_network
	"next" -> R.drawable.ic_next
	"pause" -> R.drawable.ic_pause
	"play" -> R.drawable.ic_play
	"power" -> R.drawable.ic_power
	"previous" -> R.drawable.ic_previous
	"processes" -> R.drawable.ic_processes
	"restart" -> R.drawable.ic_restart
	"rocket" -> R.drawable.ic_rocket
	"storage" -> R.drawable.ic_storage
	"terminal" -> R.drawable.ic_terminal
	"thermometer" -> R.drawable.ic_thermometer
	"volume" -> R.drawable.ic_volume
	else -> R.drawable.ic_overview
}

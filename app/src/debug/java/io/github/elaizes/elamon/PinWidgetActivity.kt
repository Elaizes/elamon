package io.github.elaizes.elamon

import android.app.Activity
import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.os.Bundle
import io.github.elaizes.elamon.widget.ElamonWidgetReceiver

/**
 * Debug builds only, for maestro/run.sh: asks the home screen to put the widget on it, as a person
 * would by dragging it from the list of widgets, which a test can't. The home screen asks first.
 */
class PinWidgetActivity : Activity() {
	override fun onCreate(savedInstanceState: Bundle?) {
		super.onCreate(savedInstanceState)
		val manager = getSystemService(AppWidgetManager::class.java)
		if (manager.isRequestPinAppWidgetSupported) {
			manager.requestPinAppWidget(ComponentName(this, ElamonWidgetReceiver::class.java), null, null)
		}
		finish()
	}
}

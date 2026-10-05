package com.iptellytv

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import org.json.JSONObject
import java.io.File

// Starting the app by itself: after the TV boots, and when it wakes from
// sleep, if Settings > General asks for it. Android only lets an app open
// from the background once it may "display over other apps", which the
// settings screen asks for when either is turned on.
object AutoStart {
  // A setting from settings.json, which src/appSettings.ts saves in the
  // app's files folder.
  fun enabled(context: Context, name: String): Boolean =
    try {
      JSONObject(File(context.filesDir, "settings.json").readText()).optBoolean(name, false)
    } catch (e: Exception) {
      false
    }

  fun launch(context: Context) {
    val intent =
      Intent(context, MainActivity::class.java)
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_REORDER_TO_FRONT)
    try {
      context.startActivity(intent)
    } catch (e: Exception) {
      // Not allowed to open from the background on this device.
    }
  }

  // Waking from sleep turns the screen on. Android only tells running apps,
  // so this only works while the app is still in memory.
  fun listenForWakeUp(context: Context) {
    val receiver =
      object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
          if (enabled(context, "autoStartOnWake")) {
            launch(context)
          }
        }
      }
    val filter = IntentFilter(Intent.ACTION_SCREEN_ON)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
    } else {
      context.registerReceiver(receiver, filter)
    }
  }
}

class BootReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    if (intent.action == Intent.ACTION_BOOT_COMPLETED &&
      AutoStart.enabled(context, "autoStartOnBoot")
    ) {
      AutoStart.launch(context)
    }
  }
}

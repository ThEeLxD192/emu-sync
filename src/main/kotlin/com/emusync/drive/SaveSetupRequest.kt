package com.emusync.drive

import com.emusync.model.SaveTarget
import com.emusync.ui.GameItem

internal typealias SaveSetupRequest =
    suspend (item: GameItem, notice: String?, cloudFileNames: List<String>) -> List<SaveTarget>

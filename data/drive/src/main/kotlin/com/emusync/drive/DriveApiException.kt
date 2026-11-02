package com.emusync.drive

class DriveApiException(message: String, cause: Throwable? = null, val errorCode: String? = null) :
    Exception(message, cause)

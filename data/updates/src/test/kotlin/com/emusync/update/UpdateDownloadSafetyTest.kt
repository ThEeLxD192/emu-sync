package com.emusync.update

import com.emusync.domain.update.UpdateInfo
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertSame
import kotlin.test.assertTrue
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class UpdateDownloadSafetyTest {
    @Test
    fun `download without content length preserves all bytes and reports completion`(
        @TempDir directory: File
    ) = runTest {
        val content = "COMPLETE UPDATE"
        val destination = File(directory, "update.AppImage")
        val progress = mutableListOf<Float>()
        HttpClient(MockEngine { respond(content) }).use { client ->
            val result =
                GitHubUpdateSource(client, "owner", "repo", "1.0.0", false)
                    .download(
                        UpdateInfo(
                            "1",
                            "v1",
                            "",
                            "https://updates.example/file",
                            0,
                        ),
                        destination.absolutePath,
                        progress::add,
                    )

            assertTrue(result.isSuccess)
            assertEquals(content, destination.readText())
            assertEquals(listOf(1f), progress)
            assertEquals(listOf(destination.name), directory.list()?.toList())
        }
    }

    @Test
    fun `progress callback failure retains the previous download and removes staging files`(
        @TempDir directory: File
    ) = runTest {
        val destination = File(directory, "update.AppImage").apply { writeText("PREVIOUS") }
        val failure = IllegalStateException("Progress callback failed")
        HttpClient(
                MockEngine {
                    respond("UPDATE", headers = headersOf(HttpHeaders.ContentLength, "6"))
                }
            )
            .use { client ->
                val result =
                    GitHubUpdateSource(client, "owner", "repo", "1.0.0", false).download(
                        UpdateInfo(
                            "1",
                            "v1",
                            "",
                            "https://updates.example/file",
                            0,
                        ),
                        destination.absolutePath,
                    ) {
                        throw failure
                    }

                assertSame(failure, result.exceptionOrNull())
                assertEquals("PREVIOUS", destination.readText())
                assertEquals(listOf(destination.name), directory.list()?.toList())
            }
    }

    @Test
    fun `cancellation during progress reporting escapes and preserves the previous download`(
        @TempDir directory: File
    ) = runTest {
        val destination = File(directory, "update.AppImage").apply { writeText("PREVIOUS") }
        val cancellation = CancellationException("Cancel update while copying")
        HttpClient(
                MockEngine {
                    respond("UPDATE", headers = headersOf(HttpHeaders.ContentLength, "6"))
                }
            )
            .use { client ->
                val thrown =
                    assertFailsWith<CancellationException> {
                        GitHubUpdateSource(client, "owner", "repo", "1.0.0", false).download(
                            UpdateInfo(
                                "1",
                                "v1",
                                "",
                                "https://updates.example/file",
                                0,
                            ),
                            destination.absolutePath,
                        ) {
                            throw cancellation
                        }
                    }

                assertEquals(cancellation.message, thrown.message)
                assertEquals("PREVIOUS", destination.readText())
                assertEquals(listOf(destination.name), directory.list()?.toList())
            }
    }

    @Test
    fun `truncated update retains an existing download and removes staging files`(
        @TempDir directory: File
    ) = runTest {
        val destination =
            File(directory, "update.AppImage").apply { writeText("PREVIOUS VALID DOWNLOAD") }
        HttpClient(
                MockEngine {
                    respond("SHORT", headers = headersOf(HttpHeaders.ContentLength, "100"))
                }
            )
            .use { client ->
                val result =
                    GitHubUpdateSource(client, "owner", "repo", "1.0.0", false).download(
                        UpdateInfo(
                            "1",
                            "v1",
                            "",
                            "https://updates.example/file",
                            0,
                        ),
                        destination.absolutePath,
                    ) {}
                assertTrue(result.isFailure)
                assertEquals("PREVIOUS VALID DOWNLOAD", destination.readText())
                assertEquals(listOf(destination.name), directory.listFiles()!!.map { it.name })
            }
    }

    @Test
    fun `empty and rejected downloads never replace a previous file`(@TempDir directory: File) =
        runTest {
            for (status in listOf(HttpStatusCode.OK, HttpStatusCode.ServiceUnavailable)) {
                val destination = File(directory, "update.AppImage").apply { writeText("PREVIOUS") }
                HttpClient(MockEngine { respond("", status = status) }).use { client ->
                    assertTrue(
                        GitHubUpdateSource(client, "owner", "repo", "1.0.0", false)
                            .download(
                                UpdateInfo(
                                    "1",
                                    "v1",
                                    "",
                                    "https://updates.example/file",
                                    0,
                                ),
                                destination.absolutePath,
                            ) {}
                            .isFailure
                    )
                    assertEquals("PREVIOUS", destination.readText())
                }
            }
            assertEquals(1, directory.listFiles()!!.size)
        }

    @Test
    fun `cancelled download propagates cancellation and preserves destination`(
        @TempDir directory: File
    ) = runTest {
        val destination = File(directory, "update.AppImage").apply { writeText("PREVIOUS") }
        val requested = CompletableDeferred<Unit>()
        HttpClient(
                MockEngine {
                    requested.complete(Unit)
                    awaitCancellation()
                }
            )
            .use { client ->
                val task = async {
                    GitHubUpdateSource(client, "owner", "repo", "1.0.0", false).download(
                        UpdateInfo(
                            "1",
                            "v1",
                            "",
                            "https://updates.example/file",
                            0,
                        ),
                        destination.absolutePath,
                    ) {}
                }
                requested.await()
                task.cancelAndJoin()
                assertTrue(task.isCancelled)
                assertEquals("PREVIOUS", destination.readText())
                assertEquals(listOf(destination.name), directory.listFiles()!!.map { it.name })
            }
    }
}

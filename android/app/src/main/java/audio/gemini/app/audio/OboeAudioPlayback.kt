package audio.gemini.app.audio

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.os.Process
import java.util.concurrent.LinkedBlockingQueue

/**
 * Handles audio playback to the speaker using Android's AudioTrack API.
 * Plays 16-bit signed PCM at 24000 Hz (or configured sample rate), mono.
 *
 * Key design decisions (based on Android audio best practices):
 * - Buffer = 8x minBufferSize to absorb network jitter from WebSocket streaming
 * - LinkedBlockingQueue.take() instead of polling + sleep — no timing jitter
 * - THREAD_PRIORITY_AUDIO so the OS scheduler treats this as a real audio thread
 * - Empty ByteArray sentinel for clean shutdown (LinkedBlockingQueue rejects null)
 */

private val STOP_SENTINEL = ByteArray(0)
class OboeAudioPlayback(
    private val sampleRate: Int = 24000,
) {
    private val minBufSize = AudioTrack.getMinBufferSize(
        sampleRate,
        AudioFormat.CHANNEL_OUT_MONO,
        AudioFormat.ENCODING_PCM_16BIT,
    )
    private val trackBufSize = minBufSize * 8

    private var audioTrack: AudioTrack? = null
    private val queue = LinkedBlockingQueue<ByteArray>()
    private var playbackThread: Thread? = null

    fun start() {
        if (playbackThread?.isAlive == true) return

        audioTrack = AudioTrack.Builder()
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                    .build()
            )
            .setAudioFormat(
                AudioFormat.Builder()
                    .setSampleRate(sampleRate)
                    .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                    .setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
                    .build()
            )
            .setBufferSizeInBytes(trackBufSize)
            .setTransferMode(AudioTrack.MODE_STREAM)
            .setSessionId(AudioManager.AUDIO_SESSION_ID_GENERATE)
            .build()

        if (audioTrack?.state != AudioTrack.STATE_INITIALIZED) {
            audioTrack?.release()
            audioTrack = null
            return
        }

        playbackThread = Thread {
            Process.setThreadPriority(Process.THREAD_PRIORITY_AUDIO)
            val track = audioTrack ?: return@Thread
            track.play()

            while (true) {
                val chunk = queue.take()
                if (chunk === STOP_SENTINEL) break
                track.write(chunk, 0, chunk.size, AudioTrack.WRITE_BLOCKING)
            }

            track.stop()
            track.release()
            audioTrack = null
        }
        playbackThread?.start()
    }

    fun write(chunk: ByteArray) {
        queue.put(chunk)
    }

    /** Graceful stop — drains remaining audio before exiting. */
    fun stop() {
        queue.put(STOP_SENTINEL)
        playbackThread?.join(2000)
        playbackThread = null
        queue.clear()
    }

    /**
     * Immediate stop for barge-in — cuts audio instantly without blocking the caller.
     * Pauses the AudioTrack hardware, clears queued chunks, then signals the thread to exit.
     */
    fun stopImmediate() {
        audioTrack?.pause()
        queue.clear()
        queue.put(STOP_SENTINEL)
        // Don't join — let the thread clean up asynchronously
        playbackThread = null
    }

    fun isPlaying(): Boolean = playbackThread?.isAlive == true
}

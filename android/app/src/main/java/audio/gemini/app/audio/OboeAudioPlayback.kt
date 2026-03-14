package audio.gemini.app.audio

import android.media.AudioFormat
import android.media.AudioTrack
import android.media.VolumeShaper
import android.os.Process
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Handles audio playback to the speaker using Android's AudioTrack API.
 * Plays 16-bit signed PCM at 24000 Hz, mono (or 16000 Hz depending on input).
 * Uses VolumeShaper to prevent popping at start/end.
 */
class OboeAudioPlayback(
    private val sampleRate: Int = 16000,
    private val channelCount: Int = 1
) {
    private var audioTrack: AudioTrack? = null
    private var volumeShaper: VolumeShaper? = null
    private val isPlaying = AtomicBoolean(false)
    private var playbackThread: Thread? = null
    private val audioQueue = ConcurrentLinkedQueue<ByteArray>()

    fun start() {
        if (isPlaying.get()) return

        try {
            val channelConfig = if (channelCount == 1) AudioFormat.CHANNEL_OUT_MONO else AudioFormat.CHANNEL_OUT_STEREO
            val audioFormat = AudioFormat.ENCODING_PCM_16BIT
            val bufferSize = AudioTrack.getMinBufferSize(sampleRate, channelConfig, audioFormat)

            audioTrack = AudioTrack(
                android.media.AudioManager.STREAM_MUSIC,
                sampleRate,
                channelConfig,
                audioFormat,
                bufferSize,
                AudioTrack.MODE_STREAM
            )

            if (audioTrack?.state != AudioTrack.STATE_INITIALIZED) {
                throw RuntimeException("AudioTrack failed to initialize")
            }

            // Create VolumeShaper for fade-in (prevents popping at start)
            // Fade in over 50ms - enough to prevent click but not noticeable
            val fadeConfig = VolumeShaper.Configuration.Builder()
                .setDuration(50)  // 50ms fade
                .setCurve(floatArrayOf(0f, 1f), floatArrayOf(0f, 1f))
                .setInterpolatorType(VolumeShaper.Configuration.INTERPOLATOR_TYPE_LINEAR)
                .build()
            
            volumeShaper = audioTrack?.createVolumeShaper(fadeConfig)

            isPlaying.set(true)
            playbackThread = Thread {
                Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
                audioTrack?.play()
                
                // Apply fade-in
                volumeShaper?.apply(VolumeShaper.Operation.PLAY)

                while (isPlaying.get()) {
                    val data = audioQueue.poll()
                    if (data != null) {
                        audioTrack?.write(data, 0, data.size)
                    } else {
                        // Sleep briefly to avoid busy-waiting
                        Thread.sleep(10)
                    }
                }

                // Apply fade-out before stopping to prevent popping
                applyFadeOut()

                audioTrack?.stop()
                audioTrack?.release()
                audioTrack = null
                volumeShaper?.close()
                volumeShaper = null
            }
            playbackThread?.start()
        } catch (e: Exception) {
            e.printStackTrace()
            isPlaying.set(false)
        }
    }

    private fun applyFadeOut() {
        try {
            // Quick fade out over 30ms to prevent click at end
            val fadeConfig = VolumeShaper.Configuration.Builder()
                .setDuration(30)
                .setCurve(floatArrayOf(0f, 1f), floatArrayOf(1f, 0f))
                .setInterpolatorType(VolumeShaper.Configuration.INTERPOLATOR_TYPE_LINEAR)
                .build()
            
            val fadeOutShaper = audioTrack?.createVolumeShaper(fadeConfig)
            fadeOutShaper?.apply(VolumeShaper.Operation.PLAY)
            
            // Wait for fade to complete
            Thread.sleep(35)
            fadeOutShaper?.close()
        } catch (e: Exception) {
            // Ignore fade-out errors
        }
    }

    fun stop() {
        isPlaying.set(false)
        playbackThread?.join(1000)
        playbackThread = null
        audioQueue.clear()
    }

    fun write(chunk: ByteArray) {
        if (isPlaying.get()) {
            audioQueue.offer(chunk)
        }
    }

    fun isPlaying(): Boolean = isPlaying.get()
}

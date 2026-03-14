package audio.gemini.app.audio

import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.os.Process
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Handles audio capture from the microphone using Android's AudioRecord API.
 * Captures 16-bit signed PCM at 16000 Hz, mono.
 */
class OboeAudioCapture(
    private val sampleRate: Int = 16000,
    private val channelCount: Int = 1,
    private val onAudioData: (ByteArray) -> Unit
) {
    private var audioRecord: AudioRecord? = null
    private val isRecording = AtomicBoolean(false)
    private var recordingThread: Thread? = null

    fun start() {
        if (isRecording.get()) return

        try {
            val channelConfig = if (channelCount == 1) AudioFormat.CHANNEL_IN_MONO else AudioFormat.CHANNEL_IN_STEREO
            val audioFormat = AudioFormat.ENCODING_PCM_16BIT
            val bufferSize = AudioRecord.getMinBufferSize(sampleRate, channelConfig, audioFormat)

            audioRecord = AudioRecord(
                MediaRecorder.AudioSource.MIC,
                sampleRate,
                channelConfig,
                audioFormat,
                bufferSize
            )

            if (audioRecord?.state != AudioRecord.STATE_INITIALIZED) {
                throw RuntimeException("AudioRecord failed to initialize")
            }

            isRecording.set(true)
            recordingThread = Thread {
                Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
                val buffer = ByteArray(bufferSize)
                audioRecord?.startRecording()

                while (isRecording.get()) {
                    val read = audioRecord?.read(buffer, 0, bufferSize) ?: -1
                    if (read > 0) {
                        onAudioData(buffer.copyOf(read))
                    }
                }

                audioRecord?.stop()
                audioRecord?.release()
                audioRecord = null
            }
            recordingThread?.start()
        } catch (e: Exception) {
            e.printStackTrace()
            isRecording.set(false)
        }
    }

    fun stop() {
        isRecording.set(false)
        recordingThread?.join(1000)
        recordingThread = null
    }
}

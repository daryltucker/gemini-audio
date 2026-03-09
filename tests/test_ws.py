import asyncio
import websockets
import json
import os
import base64

API_KEY = os.environ.get("GEMINI_API_KEY")
URL = f"wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key={API_KEY}"

async def run():
    async with websockets.connect(URL) as ws:
        setup = {
            "setup": {
                "model": "models/gemini-2.5-flash-native-audio-preview-12-2025",
                "generationConfig": {"responseModalities": ["AUDIO"]}
            }
        }
        await ws.send(json.dumps(setup))
        res = await ws.recv()
        
        # Send end of turn with an empty text part
        end_turn = {
            "clientContent": {
                "turns": [{"role": "user", "parts": [{"text": "hello"}]}],
                "turnComplete": True
            }
        }
        print("Sending:", end_turn)
        await ws.send(json.dumps(end_turn))
        
        try:
            res = await ws.recv()
            print("Response:", res)
        except Exception as e:
            print("Error:", e)

asyncio.run(run())

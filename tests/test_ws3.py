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
                "model": "models/gemini-2.0-flash-exp",
            }
        }
        await ws.send(json.dumps(setup))
        res = await ws.recv()
        print("Setup res:", res)
        
        audio = {
            "realtimeInput": {
                "mediaChunks": [{"mimeType": "audio/pcm;rate=16000", "data": base64.b64encode(b"\x00"*1600).decode("utf-8")}]
            }
        }
        await ws.send(json.dumps(audio))
        
        end_turn = {
            "clientContent": {
                "turnComplete": True
            }
        }
        await ws.send(json.dumps(end_turn))
        
        try:
            res = await ws.recv()
            print("Response:", res)
        except Exception as e:
            print("Error:", e)

asyncio.run(run())

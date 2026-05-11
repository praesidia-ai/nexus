"use client";

import { useState, useRef, useCallback, useEffect } from "react";
import {
  Mic,
  Volume2,
  Square,
  Loader2,
  MessageSquare,
  Settings2,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api, type VoiceChatResponse } from "@/lib/api";
import { Badge } from "@/components/ui/badge";

type VoiceState = "idle" | "recording" | "processing" | "speaking";

interface VoiceMessage {
  id: string;
  role: "user" | "assistant";
  text: string;
  audioBase64?: string;
}

const VOICES = [
  { id: "alloy", label: "Alloy", description: "Neutral & balanced" },
  { id: "echo", label: "Echo", description: "Warm & conversational" },
  { id: "fable", label: "Fable", description: "Expressive & British" },
  { id: "onyx", label: "Onyx", description: "Deep & authoritative" },
  { id: "nova", label: "Nova", description: "Friendly & upbeat" },
  { id: "shimmer", label: "Shimmer", description: "Soft & calm" },
];

export function VoiceChat({ systemPrompt }: { systemPrompt?: string }) {
  const [state, setState] = useState<VoiceState>("idle");
  const [messages, setMessages] = useState<VoiceMessage[]>([]);
  const [voice, setVoice] = useState("nova");
  const [showSettings, setShowSettings] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [visualizerData, setVisualizerData] = useState<number[]>(new Array(32).fill(0));

  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const audioChunksRef = useRef<Blob[]>([]);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const analyserRef = useRef<AnalyserNode | null>(null);
  const animFrameRef = useRef<number>(0);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  useEffect(() => {
    return () => {
      cancelAnimationFrame(animFrameRef.current);
      if (audioRef.current) {
        audioRef.current.pause();
        audioRef.current = null;
      }
    };
  }, []);

  const updateVisualizer = useCallback(() => {
    if (analyserRef.current) {
      const data = new Uint8Array(analyserRef.current.frequencyBinCount);
      analyserRef.current.getByteFrequencyData(data);
      const normalized = Array.from(data.slice(0, 32)).map(v => v / 255);
      setVisualizerData(normalized);
    }
    animFrameRef.current = requestAnimationFrame(updateVisualizer);
  }, []);

  const startRecording = useCallback(async () => {
    try {
      setError(null);
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });

      const audioCtx = new AudioContext();
      const source = audioCtx.createMediaStreamSource(stream);
      const analyser = audioCtx.createAnalyser();
      analyser.fftSize = 64;
      source.connect(analyser);
      analyserRef.current = analyser;

      const recorder = new MediaRecorder(stream, { mimeType: "audio/webm;codecs=opus" });
      audioChunksRef.current = [];

      recorder.ondataavailable = (e) => {
        if (e.data.size > 0) audioChunksRef.current.push(e.data);
      };

      recorder.onstop = async () => {
        stream.getTracks().forEach(t => t.stop());
        cancelAnimationFrame(animFrameRef.current);
        setVisualizerData(new Array(32).fill(0));
        audioCtx.close();

        const blob = new Blob(audioChunksRef.current, { type: "audio/webm" });
        const reader = new FileReader();
        reader.onloadend = async () => {
          const b64 = (reader.result as string).split(",")[1];
          await processVoiceInput(b64);
        };
        reader.readAsDataURL(blob);
      };

      recorder.start(100);
      mediaRecorderRef.current = recorder;
      setState("recording");
      updateVisualizer();
    } catch {
      setError("Microphone access denied. Please allow microphone permissions.");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [updateVisualizer]);

  const stopRecording = useCallback(() => {
    if (mediaRecorderRef.current?.state === "recording") {
      mediaRecorderRef.current.stop();
      setState("processing");
    }
  }, []);

  async function processVoiceInput(audioBase64: string) {
    setState("processing");

    const conversationHistory = messages.map(m => ({
      role: m.role,
      content: m.text,
    }));

    try {
      const resp: VoiceChatResponse = await api.voiceChat({
        audio_base64: audioBase64,
        mime_type: "audio/webm",
        system_prompt: systemPrompt,
        voice,
        messages: conversationHistory,
      });

      const userMsg: VoiceMessage = {
        id: `user-${Date.now()}`,
        role: "user",
        text: resp.transcript,
      };
      const assistantMsg: VoiceMessage = {
        id: `asst-${Date.now()}`,
        role: "assistant",
        text: resp.response_text,
        audioBase64: resp.response_audio_base64,
      };

      setMessages(prev => [...prev, userMsg, assistantMsg]);
      setState("speaking");
      playAudio(resp.response_audio_base64);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Voice pipeline failed");
      setState("idle");
    }
  }

  function playAudio(base64: string) {
    const audio = new Audio(`data:audio/mp3;base64,${base64}`);
    audioRef.current = audio;
    audio.onended = () => setState("idle");
    audio.onerror = () => setState("idle");
    audio.play().catch(() => setState("idle"));
  }

  function stopAudio() {
    if (audioRef.current) {
      audioRef.current.pause();
      audioRef.current.currentTime = 0;
      audioRef.current = null;
    }
    setState("idle");
  }

  const stateColors: Record<VoiceState, string> = {
    idle: "from-slate-500/20 to-slate-500/10",
    recording: "from-red-500/30 to-red-500/10",
    processing: "from-blue-500/30 to-blue-500/10",
    speaking: "from-emerald-500/30 to-emerald-500/10",
  };

  const stateLabels: Record<VoiceState, string> = {
    idle: "Tap to speak",
    recording: "Listening...",
    processing: "Thinking...",
    speaking: "Speaking...",
  };

  return (
    <div className="flex flex-col h-full">
      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-4 py-4 space-y-3">
        {messages.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full text-slate-500 gap-3">
            <div className="w-16 h-16 rounded-full bg-gradient-to-br from-violet-500/20 to-blue-500/20 flex items-center justify-center">
              <Mic className="w-7 h-7 text-violet-400" />
            </div>
            <p className="text-sm font-medium">Voice Chat</p>
            <p className="text-xs text-center max-w-xs">
              Press the microphone button and speak. Your voice will be transcribed,
              processed by AI, and the response will be spoken back.
            </p>
          </div>
        )}

        {messages.map(m => (
          <div
            key={m.id}
            className={cn(
              "flex gap-3 max-w-[80%]",
              m.role === "user" ? "ml-auto flex-row-reverse" : "",
            )}
          >
            <div
              className={cn(
                "rounded-2xl px-4 py-2.5 text-sm",
                m.role === "user"
                  ? "bg-blue-600/30 text-white rounded-br-sm"
                  : "bg-white/[0.06] text-slate-200 rounded-bl-sm",
              )}
            >
              {m.text}
              {m.role === "assistant" && m.audioBase64 && (
                <button
                  onClick={() => playAudio(m.audioBase64!)}
                  className="mt-1.5 flex items-center gap-1 text-[10px] text-slate-400 hover:text-white transition-colors"
                >
                  <Volume2 className="w-3 h-3" /> Replay
                </button>
              )}
            </div>
          </div>
        ))}
        <div ref={messagesEndRef} />
      </div>

      {/* Error */}
      {error && (
        <div className="mx-4 mb-2 p-2 rounded-lg bg-red-500/10 border border-red-500/20 text-xs text-red-300">
          {error}
        </div>
      )}

      {/* Controls */}
      <div className="border-t border-white/[0.06] px-4 py-4">
        {/* Visualizer */}
        <div className="flex items-end justify-center gap-[2px] h-8 mb-3">
          {visualizerData.map((v, i) => (
            <div
              key={i}
              className={cn(
                "w-1.5 rounded-full transition-all duration-75",
                state === "recording" ? "bg-red-400" : "bg-white/10",
              )}
              style={{ height: `${Math.max(2, v * 32)}px` }}
            />
          ))}
        </div>

        <div className="flex items-center justify-center gap-4">
          {/* Settings */}
          <button
            onClick={() => setShowSettings(!showSettings)}
            className="w-10 h-10 rounded-full flex items-center justify-center text-slate-400 hover:text-white hover:bg-white/[0.06] transition-colors"
          >
            <Settings2 className="w-5 h-5" />
          </button>

          {/* Main button */}
          {state === "idle" && (
            <button
              onClick={startRecording}
              className={cn(
                "w-16 h-16 rounded-full flex items-center justify-center transition-all",
                "bg-gradient-to-br",
                stateColors.idle,
                "hover:from-violet-500/40 hover:to-blue-500/20 border border-white/10",
              )}
            >
              <Mic className="w-7 h-7 text-white" />
            </button>
          )}
          {state === "recording" && (
            <button
              onClick={stopRecording}
              className={cn(
                "w-16 h-16 rounded-full flex items-center justify-center transition-all animate-pulse",
                "bg-gradient-to-br",
                stateColors.recording,
                "border border-red-500/30",
              )}
            >
              <Square className="w-6 h-6 text-red-300" />
            </button>
          )}
          {state === "processing" && (
            <div className={cn(
              "w-16 h-16 rounded-full flex items-center justify-center",
              "bg-gradient-to-br",
              stateColors.processing,
              "border border-blue-500/20",
            )}>
              <Loader2 className="w-7 h-7 text-blue-300 animate-spin" />
            </div>
          )}
          {state === "speaking" && (
            <button
              onClick={stopAudio}
              className={cn(
                "w-16 h-16 rounded-full flex items-center justify-center transition-all",
                "bg-gradient-to-br",
                stateColors.speaking,
                "border border-emerald-500/20",
              )}
            >
              <Volume2 className="w-7 h-7 text-emerald-300 animate-pulse" />
            </button>
          )}

          {/* Messages count */}
          <div className="w-10 h-10 rounded-full flex items-center justify-center text-slate-400">
            <div className="relative">
              <MessageSquare className="w-5 h-5" />
              {messages.length > 0 && (
                <Badge className="absolute -top-2 -right-3 text-[9px] h-4 min-w-4 flex items-center justify-center p-0 bg-violet-600">
                  {messages.length}
                </Badge>
              )}
            </div>
          </div>
        </div>

        <p className="text-center text-[11px] text-slate-500 mt-2">{stateLabels[state]}</p>

        {/* Voice settings */}
        {showSettings && (
          <div className="mt-3 pt-3 border-t border-white/[0.06]">
            <p className="text-xs text-slate-400 mb-2">Voice</p>
            <div className="grid grid-cols-3 gap-1.5">
              {VOICES.map(v => (
                <button
                  key={v.id}
                  onClick={() => setVoice(v.id)}
                  className={cn(
                    "text-left px-3 py-2 rounded-lg text-xs transition-colors border",
                    voice === v.id
                      ? "bg-violet-600/20 border-violet-500/30 text-white"
                      : "bg-white/[0.03] border-white/[0.06] text-slate-400 hover:bg-white/[0.06]",
                  )}
                >
                  <p className="font-medium">{v.label}</p>
                  <p className="text-[10px] opacity-60">{v.description}</p>
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

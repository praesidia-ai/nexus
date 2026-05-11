import React, { useCallback, useEffect, useRef, useState } from 'react';

export interface NexusWidgetConfig {
  /** URL of the Nexus server, e.g. "https://api.example.com" */
  serverUrl: string;
  /** Optional API key for authenticated endpoints */
  apiKey?: string;
  /** Project ID to scope conversations to */
  projectId?: string;
  /** Widget title shown in the header */
  title?: string;
  /** Initial placeholder text for the input */
  placeholder?: string;
  /** Accent color (CSS color) */
  accentColor?: string;
  /** Whether to start open */
  defaultOpen?: boolean;
  /** Called when the user submits a message */
  onMessage?: (message: string) => void;
  /** Called when an agent reply is received */
  onReply?: (reply: string) => void;
}

interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: Date;
  streaming?: boolean;
}

function uid(): string {
  return Math.random().toString(36).slice(2);
}

export function NexusWidget(props: NexusWidgetConfig) {
  const {
    serverUrl,
    apiKey,
    projectId,
    title = 'Nexus AI',
    placeholder = 'Ask anything…',
    accentColor = '#7c3aed',
    defaultOpen = false,
    onMessage,
    onReply,
  } = props;

  const [open, setOpen] = useState(defaultOpen);
  const [input, setInput] = useState('');
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [loading, setLoading] = useState(false);
  const endRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const sendMessage = useCallback(async () => {
    const text = input.trim();
    if (!text || loading) return;

    setInput('');
    onMessage?.(text);

    const userMsg: ChatMessage = { id: uid(), role: 'user', content: text, timestamp: new Date() };
    setMessages((prev) => [...prev, userMsg]);

    const assistantId = uid();
    setMessages((prev) => [
      ...prev,
      { id: assistantId, role: 'assistant', content: '', timestamp: new Date(), streaming: true },
    ]);
    setLoading(true);

    try {
      const endpoint = projectId
        ? `${serverUrl}/projects/${projectId}/chat`
        : `${serverUrl}/chat`;

      const headers: Record<string, string> = { 'Content-Type': 'application/json' };
      if (apiKey) headers['Authorization'] = `Bearer ${apiKey}`;

      const response = await fetch(endpoint, {
        method: 'POST',
        headers,
        body: JSON.stringify({
          messages: [
            ...messages.map((m) => ({ role: m.role, content: m.content })),
            { role: 'user', content: text },
          ],
        }),
      });

      if (!response.ok) {
        throw new Error(`Server error: ${response.status}`);
      }

      const contentType = response.headers.get('content-type') ?? '';

      if (contentType.includes('text/event-stream')) {
        // SSE streaming
        const reader = response.body?.getReader();
        const decoder = new TextDecoder();
        let accumulated = '';

        if (reader) {
          while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            const chunk = decoder.decode(value, { stream: true });
            for (const line of chunk.split('\n')) {
              if (line.startsWith('data: ')) {
                const data = line.slice(6).trim();
                if (data === '[DONE]') break;
                try {
                  const json = JSON.parse(data);
                  const delta =
                    json.choices?.[0]?.delta?.content ??
                    json.content ??
                    json.text ??
                    '';
                  accumulated += delta;
                  setMessages((prev) =>
                    prev.map((m) =>
                      m.id === assistantId ? { ...m, content: accumulated } : m,
                    ),
                  );
                } catch {
                  // non-JSON SSE line
                }
              }
            }
          }
        }
        onReply?.(accumulated);
      } else {
        // Regular JSON response
        const json = await response.json();
        const reply =
          json.content ??
          json.message ??
          json.choices?.[0]?.message?.content ??
          JSON.stringify(json);
        setMessages((prev) =>
          prev.map((m) => (m.id === assistantId ? { ...m, content: reply, streaming: false } : m)),
        );
        onReply?.(reply);
      }
    } catch (err) {
      const errMsg = err instanceof Error ? err.message : 'Unknown error';
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantId ? { ...m, content: `Error: ${errMsg}`, streaming: false } : m,
        ),
      );
    } finally {
      setLoading(false);
      setMessages((prev) =>
        prev.map((m) => (m.id === assistantId ? { ...m, streaming: false } : m)),
      );
    }
  }, [input, loading, messages, serverUrl, projectId, apiKey, onMessage, onReply]);

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  }

  const s = styles(accentColor);

  if (!open) {
    return (
      <button onClick={() => setOpen(true)} style={s.fab} aria-label="Open Nexus AI">
        <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 17h-2v-2h2v2zm2.07-7.75-.9.92C13.45 12.9 13 13.5 13 15h-2v-.5c0-1.1.45-2.1 1.17-2.83l1.24-1.26c.37-.36.59-.86.59-1.41 0-1.1-.9-2-2-2s-2 .9-2 2H8c0-2.21 1.79-4 4-4s4 1.79 4 4c0 .88-.36 1.68-.93 2.25z" />
        </svg>
      </button>
    );
  }

  return (
    <div style={s.container}>
      {/* Header */}
      <div style={s.header}>
        <span style={s.headerTitle}>{title}</span>
        <button onClick={() => setOpen(false)} style={s.closeBtn} aria-label="Close">
          ✕
        </button>
      </div>

      {/* Messages */}
      <div style={s.messages}>
        {messages.length === 0 && (
          <div style={s.emptyState}>
            <div style={{ fontSize: 32, marginBottom: 8 }}>🤖</div>
            <p style={{ margin: 0, color: '#888', fontSize: 13 }}>
              Hello! How can I help you today?
            </p>
          </div>
        )}
        {messages.map((msg) => (
          <div key={msg.id} style={msg.role === 'user' ? s.userBubbleWrapper : s.aiBubbleWrapper}>
            <div style={msg.role === 'user' ? { ...s.bubble, ...s.userBubble } : s.bubble}>
              {msg.content || (msg.streaming ? <span style={s.cursor}>▊</span> : '')}
            </div>
          </div>
        ))}
        <div ref={endRef} />
      </div>

      {/* Input */}
      <div style={s.inputRow}>
        <textarea
          ref={inputRef}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          rows={1}
          style={s.textarea}
          disabled={loading}
        />
        <button
          onClick={sendMessage}
          style={{ ...s.sendBtn, opacity: loading || !input.trim() ? 0.4 : 1 }}
          disabled={loading || !input.trim()}
          aria-label="Send"
        >
          ➤
        </button>
      </div>
    </div>
  );
}

function styles(accent: string) {
  return {
    fab: {
      position: 'fixed' as const,
      bottom: 24,
      right: 24,
      width: 52,
      height: 52,
      borderRadius: '50%',
      background: accent,
      color: '#fff',
      border: 'none',
      cursor: 'pointer',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      boxShadow: '0 4px 16px rgba(0,0,0,0.25)',
      zIndex: 9999,
    },
    container: {
      position: 'fixed' as const,
      bottom: 24,
      right: 24,
      width: 360,
      height: 520,
      background: '#1a1a2e',
      borderRadius: 16,
      boxShadow: '0 8px 32px rgba(0,0,0,0.4)',
      display: 'flex',
      flexDirection: 'column' as const,
      zIndex: 9999,
      fontFamily: "'Inter', system-ui, sans-serif",
      overflow: 'hidden',
    },
    header: {
      background: accent,
      padding: '12px 16px',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
    },
    headerTitle: {
      color: '#fff',
      fontWeight: 600,
      fontSize: 15,
    },
    closeBtn: {
      background: 'transparent',
      border: 'none',
      color: 'rgba(255,255,255,0.8)',
      cursor: 'pointer',
      fontSize: 16,
      padding: '0 4px',
    },
    messages: {
      flex: 1,
      overflowY: 'auto' as const,
      padding: '12px 16px',
      display: 'flex',
      flexDirection: 'column' as const,
      gap: 10,
    },
    emptyState: {
      flex: 1,
      display: 'flex',
      flexDirection: 'column' as const,
      alignItems: 'center',
      justifyContent: 'center',
      paddingTop: 60,
    },
    userBubbleWrapper: {
      display: 'flex',
      justifyContent: 'flex-end' as const,
    },
    aiBubbleWrapper: {
      display: 'flex',
      justifyContent: 'flex-start' as const,
    },
    bubble: {
      maxWidth: '78%',
      padding: '8px 12px',
      borderRadius: 12,
      fontSize: 13,
      lineHeight: 1.5,
      background: '#2a2a4a',
      color: '#e0e0f0',
      wordBreak: 'break-word' as const,
    },
    userBubble: {
      background: accent,
      color: '#fff',
    },
    cursor: {
      animation: 'blink 0.8s step-end infinite',
    },
    inputRow: {
      padding: '8px 12px',
      background: '#12122a',
      display: 'flex',
      alignItems: 'flex-end',
      gap: 8,
      borderTop: '1px solid rgba(255,255,255,0.06)',
    },
    textarea: {
      flex: 1,
      resize: 'none' as const,
      background: '#2a2a4a',
      border: '1px solid rgba(255,255,255,0.1)',
      borderRadius: 10,
      color: '#e0e0f0',
      padding: '8px 10px',
      fontSize: 13,
      outline: 'none',
      fontFamily: 'inherit',
      maxHeight: 120,
    },
    sendBtn: {
      background: accent,
      border: 'none',
      color: '#fff',
      width: 36,
      height: 36,
      borderRadius: '50%',
      cursor: 'pointer',
      fontSize: 15,
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      flexShrink: 0,
    },
  };
}

export default NexusWidget;

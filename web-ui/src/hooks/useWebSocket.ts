import { useEffect, useRef, useCallback } from 'react'
import { wsUrl } from '../api'

export type StreamEvent =
  | { type: 'content'; payload: { text: string; chunk: boolean } }
  | { type: 'progress_content'; payload: { text: string; chunk: boolean } }
  | { type: 'thinking_content'; payload: { text: string; chunk: boolean } }
  | { type: 'trace'; payload: { kind: string; [key: string]: unknown } }
  | { type: 'status'; payload: { kind: string; [key: string]: unknown } }

interface UseWebSocketOptions {
  onEvent: (event: StreamEvent, sessionId: string) => void
  onClose?: (sessionId: string) => void
  onError?: (sessionId: string) => void
}

export function useWebSocket(sessionId: string | null, opts: UseWebSocketOptions) {
  const wsRef = useRef<{ sessionId: string; socket: WebSocket } | null>(null)
  const sessionIdRef = useRef(sessionId)
  const pendingRef = useRef<Array<{ sessionId: string; payload: string }>>([])
  const optsRef = useRef(opts)
  sessionIdRef.current = sessionId
  optsRef.current = opts

  useEffect(() => {
    if (!sessionId) return

    const ws = new WebSocket(wsUrl(`/ws/sessions/${sessionId}`))
    wsRef.current = { sessionId, socket: ws }

    ws.onopen = () => {
      const remaining: typeof pendingRef.current = []
      for (const item of pendingRef.current) {
        if (item.sessionId === sessionId) {
          ws.send(item.payload)
        } else {
          remaining.push(item)
        }
      }
      pendingRef.current = remaining
    }

    ws.onmessage = (e) => {
      try {
        const event = JSON.parse(e.data) as StreamEvent
        optsRef.current.onEvent(event, sessionId)
      } catch { /* ignore parse errors */ }
    }

    ws.onerror = () => optsRef.current.onError?.(sessionId)
    ws.onclose = () => optsRef.current.onClose?.(sessionId)

    return () => {
      ws.onopen = null
      ws.onmessage = null
      ws.onerror = null
      ws.onclose = null
      if (wsRef.current?.socket === ws) wsRef.current = null
      ws.close()
    }
  }, [sessionId])

  const send = useCallback((msg: unknown, targetSessionId?: string) => {
    const destination = targetSessionId ?? sessionIdRef.current
    if (!destination) return false
    const payload = JSON.stringify(msg)
    const connection = wsRef.current
    if (connection?.sessionId === destination && connection.socket.readyState === WebSocket.OPEN) {
      connection.socket.send(payload)
    } else {
      pendingRef.current.push({ sessionId: destination, payload })
    }
    return true
  }, [])

  return { send }
}

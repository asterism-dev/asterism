// Stands in for @tauri-apps/api/core in the mobile build, where the daemon is reached over a WebSocket.
export class Channel<T> {
  onmessage: (data: T) => void = () => {};
}

export function invoke(): Promise<never> {
  return Promise.reject(new Error('Not available in the browser'));
}

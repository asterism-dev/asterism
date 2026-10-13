export async function openUrl(url: string) {
  window.open(url, '_blank', 'noopener');
}

export async function revealItemInDir() {
  throw new Error('Not available in the browser');
}

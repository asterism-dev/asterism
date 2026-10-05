export function formatBytes(bytes: number): string {
  const mb = bytes / 1024 / 1024;
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
}

export function formatCpu(percent: number): string {
  return percent >= 10 ? `${Math.round(percent)} %` : `${percent.toFixed(1)} %`;
}

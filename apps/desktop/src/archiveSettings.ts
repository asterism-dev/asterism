export type AgeUnit = 'days' | 'hours';

export const AGE_UNITS: { value: AgeUnit; label: string }[] = [
  { value: 'days', label: 'days' },
  { value: 'hours', label: 'hours' },
];

export function splitHours(hours: number): { amount: number; unit: AgeUnit } {
  return hours % 24 === 0 ? { amount: hours / 24, unit: 'days' } : { amount: hours, unit: 'hours' };
}

export const toHours = (amount: number, unit: AgeUnit) => (unit === 'days' ? amount * 24 : amount);

// Deterministic color assignment for arbitrary state names.
// Full class strings must be literals here so Tailwind's scanner includes them.
const PALETTE = [
  { dot: 'bg-gray-400',    badge: 'bg-gray-100 text-gray-700 border-gray-300' },
  { dot: 'bg-blue-400',    badge: 'bg-blue-100 text-blue-800 border-blue-300' },
  { dot: 'bg-green-500',   badge: 'bg-green-100 text-green-800 border-green-300' },
  { dot: 'bg-teal-500',    badge: 'bg-teal-100 text-teal-800 border-teal-300' },
  { dot: 'bg-purple-500',  badge: 'bg-purple-100 text-purple-800 border-purple-300' },
  { dot: 'bg-amber-400',   badge: 'bg-amber-100 text-amber-800 border-amber-300' },
  { dot: 'bg-orange-500',  badge: 'bg-orange-100 text-orange-800 border-orange-300' },
  { dot: 'bg-indigo-500',  badge: 'bg-indigo-100 text-indigo-800 border-indigo-300' },
  { dot: 'bg-emerald-500', badge: 'bg-emerald-100 text-emerald-800 border-emerald-300' },
  { dot: 'bg-rose-400',    badge: 'bg-rose-100 text-rose-800 border-rose-300' },
  { dot: 'bg-cyan-500',    badge: 'bg-cyan-100 text-cyan-800 border-cyan-300' },
  { dot: 'bg-fuchsia-500', badge: 'bg-fuchsia-100 text-fuchsia-800 border-fuchsia-300' },
]

function hash(name: string): number {
  let h = 0
  for (let i = 0; i < name.length; i++) {
    h = (h * 31 + name.charCodeAt(i)) & 0x7fffffff
  }
  return h
}

export function stateColors(name: string) {
  return PALETTE[hash(name) % PALETTE.length]
}

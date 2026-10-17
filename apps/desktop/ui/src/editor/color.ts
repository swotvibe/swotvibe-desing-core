/**
 * Colour helpers for the interface.
 *
 * Colour crosses the bridge as four channels, which is what the document
 * stores. The interface needs hex for an input and a CSS string for a swatch,
 * so both conversions live here rather than being retyped in each panel.
 */

import type { Rgba } from '@/bridge/types'

/** `#rrggbb`, the form a colour input expects. Alpha is edited separately. */
export function toHex(color: Rgba): string {
  const channel = (value: number): string => value.toString(16).padStart(2, '0')
  return `#${channel(color.r)}${channel(color.g)}${channel(color.b)}`
}

/** A CSS colour, so a swatch needs no extra markup. */
export function toCss(color: Rgba | null): string {
  if (!color) return 'transparent'
  return `rgba(${color.r}, ${color.g}, ${color.b}, ${color.a / 255})`
}

/**
 * Parses `#rgb` or `#rrggbb`.
 *
 * Returns `null` for anything else rather than guessing: a half-typed value must
 * not become a colour the document records.
 */
export function fromHex(hex: string, alpha: number): Rgba | null {
  const value = hex.trim().replace(/^#/, '')
  const expanded =
    value.length === 3
      ? value
          .split('')
          .map((character) => character + character)
          .join('')
      : value
  if (!/^[0-9a-fA-F]{6}$/.test(expanded)) return null
  return {
    r: Number.parseInt(expanded.slice(0, 2), 16),
    g: Number.parseInt(expanded.slice(2, 4), 16),
    b: Number.parseInt(expanded.slice(4, 6), 16),
    a: alpha,
  }
}

/** Alpha as the percentage an interface shows. */
export function alphaPercent(color: Rgba): number {
  return Math.round((color.a / 255) * 100)
}

/** Converts a percentage back to a channel value. */
export function alphaChannel(percent: number): number {
  return Math.min(255, Math.max(0, Math.round((percent / 100) * 255)))
}

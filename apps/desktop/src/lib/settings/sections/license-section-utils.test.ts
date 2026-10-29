import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setFormatLocaleForTests, _setLocaleForTests } from '#lib/intl/locale.js'
import { getLicenseTypeLabel, formatLicenseDate, getStatusText } from './license-section-utils'
import type { LicenseInfo, LicenseStatus } from '#lib/tauri-commands/index.js'

// The type/status labels resolve through the i18n catalog (`tString`); pin the
// base locale so the asserted en copy is deterministic.
beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

function makeLicenseInfo(overrides: Partial<LicenseInfo> = {}): LicenseInfo {
  return {
    email: 'test@example.com',
    transactionId: 'tx_123',
    issuedAt: '2025-01-15',
    organizationName: null,
    licenseType: null,
    shortCode: null,
    expiresAt: null,
    ...overrides,
  }
}

describe('getLicenseTypeLabel', () => {
  it('should return "Personal (free)" when licenseInfo is null', () => {
    expect(getLicenseTypeLabel(null)).toBe('Personal (free)')
  })

  it('should return "Personal (free)" when licenseType is null', () => {
    expect(getLicenseTypeLabel(makeLicenseInfo({ licenseType: null }))).toBe('Personal (free)')
  })

  it('should return "Commercial perpetual" for commercial_perpetual', () => {
    expect(getLicenseTypeLabel(makeLicenseInfo({ licenseType: 'commercial_perpetual' }))).toBe('Commercial perpetual')
  })

  it('should return "Commercial subscription" for commercial_subscription', () => {
    expect(getLicenseTypeLabel(makeLicenseInfo({ licenseType: 'commercial_subscription' }))).toBe(
      'Commercial subscription',
    )
  })

  it('should return "Personal (free)" for unknown licenseType', () => {
    expect(getLicenseTypeLabel(makeLicenseInfo({ licenseType: 'unknown_type' }))).toBe('Personal (free)')
  })
})

describe('formatLicenseDate', () => {
  it('should return empty string for null', () => {
    expect(formatLicenseDate(null)).toBe('')
  })

  it('should return empty string for undefined', () => {
    expect(formatLicenseDate(undefined)).toBe('')
  })

  it('should return empty string for empty string', () => {
    expect(formatLicenseDate('')).toBe('')
  })

  it('should format a valid ISO date string', () => {
    const result = formatLicenseDate('2025-06-15')
    // The exact output depends on locale, but it should contain the year
    expect(result).toContain('2025')
    // Should not be the raw input string (meaning Date parsing succeeded)
    expect(result).not.toBe('2025-06-15')
  })

  it('should format an ISO datetime string', () => {
    const result = formatLicenseDate('2025-06-15T12:00:00Z')
    expect(result).toContain('2025')
  })

  it('returns the raw string when it is not a date', () => {
    expect(formatLicenseDate('not-a-date')).toBe('not-a-date')
  })

  it('writes the date in the OS region, not the webview default', () => {
    // An English Mac set to the Swedish region: the webview alone resolves `en-US` and writes
    // "June 15, 2025"; the region the user set writes the day first.
    _setFormatLocaleForTests('en-SE')
    const expected = new Intl.DateTimeFormat('en-SE', { year: 'numeric', month: 'long', day: 'numeric' }).format(
      new Date('2025-06-15T12:00:00Z'),
    )
    expect(formatLicenseDate('2025-06-15T12:00:00Z')).toBe(expected)
    expect(expected).not.toBe('June 15, 2025')
    _setLocaleForTests('en-US')
  })
})

describe('getStatusText', () => {
  it('should return null when licenseStatus is null', () => {
    expect(getStatusText(null)).toBeNull()
  })

  it('should return null for personal status', () => {
    const status: LicenseStatus = { type: 'personal', showCommercialReminder: false }
    expect(getStatusText(status)).toBeNull()
  })

  it('should return expired text with formatted date', () => {
    const status: LicenseStatus = {
      type: 'expired',
      organizationName: null,
      expiredAt: '2025-03-01',
      showModal: false,
    }
    const result = getStatusText(status)
    expect(result).toMatch(/^Expired on /)
    expect(result).toContain('2025')
  })

  it('should return "Active" for commercial_perpetual without expiresAt', () => {
    const status: LicenseStatus = {
      type: 'commercial',
      licenseType: 'commercial_perpetual',
      organizationName: null,
      expiresAt: null,
    }
    expect(getStatusText(status)).toBe('Active')
  })

  it('should return "Updates until ..." for commercial_perpetual with expiresAt', () => {
    const status: LicenseStatus = {
      type: 'commercial',
      licenseType: 'commercial_perpetual',
      organizationName: 'Acme Corp',
      expiresAt: '2026-01-01',
    }
    const result = getStatusText(status)
    expect(result).toMatch(/^Updates until /)
    expect(result).toContain('2026')
  })

  it('should return "Active" for commercial_subscription without expiresAt', () => {
    const status: LicenseStatus = {
      type: 'commercial',
      licenseType: 'commercial_subscription',
      organizationName: null,
      expiresAt: null,
    }
    expect(getStatusText(status)).toBe('Active')
  })

  it('should return "Valid until ..." for commercial_subscription with expiresAt', () => {
    const status: LicenseStatus = {
      type: 'commercial',
      licenseType: 'commercial_subscription',
      organizationName: null,
      expiresAt: '2026-06-15',
    }
    const result = getStatusText(status)
    expect(result).toMatch(/^Valid until /)
    expect(result).toContain('2026')
  })
})

#!/usr/bin/env node
// Mock server for all FLEX API endpoints defined in live_resources/endpoints/*.md
// Rewrites all URLs: https://flex.account.gov.uk → http://localhost:8787
//
// Usage: node mock-server.js [port]
'use strict'

const http = require('node:http')

const PORT = parseInt(process.argv[2] ?? '8127', 10)

// ── Shared mock data ──────────────────────────────────────────────────────────

const USER = {
  userId: 'usr-7f3a9c1d-2e4b-6a8f',
  consentStatus: 'accepted',
  pushId: 'push-fcm-a1b2c3d4e5f6',
}

const NOTIFICATIONS = [
  {
    NotificationID: 'notif-001',
    NotificationTitle: 'DVLA Licence Renewal Reminder',
    NotificationBody: 'Your driving licence is due for renewal. Please visit GOV.UK to renew.',
    MessageTitle: 'Licence Renewal',
    MessageBody: 'Renew by 11 March 2025 to avoid a penalty.',
    DispatchedDateTime: '2026-06-14T09:00:00Z',
    Status: 'RECEIVED',
  },
  {
    NotificationID: 'notif-002',
    NotificationTitle: 'MOT Due',
    NotificationBody: 'Your vehicle AB23CDX MOT is due on 30 June 2025.',
    MessageTitle: 'MOT Reminder',
    MessageBody: 'Book your MOT before 30 June 2025.',
    DispatchedDateTime: '2026-06-01T08:30:00Z',
    Status: 'READ',
  },
]

const DRIVER = {
  dln: 'MORGA753116SM9IJ',
  firstName: 'Sarah',
  lastName: 'Morgan',
  gender: 'F',
  dateOfBirth: '1975-03-11',
  address: { line1: '12 Elm Street', line2: '', town: 'Bristol', county: '', postcode: 'BS1 5AU' },
}

const SHARE_CODE = {
  shareCodeId: 'sc-001-dvla',
  shareCodeType: 'DRIVING_LICENCE',
  createdAt: '2026-07-01T10:00:00Z',
  expiresAt: '2026-07-08T10:00:00Z',
  shareCodeStatus: 'ACTIVE',
}

const LOCAL_AUTHORITY = {
  id: 'E07000189',
  name: 'South Gloucestershire',
  homepage_url: 'https://www.southglos.gov.uk',
  tier: 'unitary',
  slug: 'south-gloucestershire',
  parent: null,
}

// ── Route table ───────────────────────────────────────────────────────────────
// Each entry: { method, path (RegExp), status, handler(match, body, req) }
// Routes with more-specific paths must appear before overlapping general ones.

const ROUTES = [

  // ── /udp – One Login User Data Platform ──────────────────────────────────

  {
    method: 'GET', path: /^\/udp\/v1\/users\/me$/,
    status: 200,
    handler: () => USER,
  },
  {
    method: 'GET', path: /^\/udp\/v1\/users\/push-id$/,
    status: 200,
    handler: () => ({ userId: USER.userId, pushId: USER.pushId }),
  },
  {
    method: 'PATCH', path: /^\/udp\/v1\/users\/me\/notifications$/,
    status: 200,
    handler: (_, body) => ({
      consentStatus: body?.consentStatus ?? USER.consentStatus,
      pushId: USER.pushId,
    }),
  },
  {
    method: 'GET', path: /^\/udp\/v1\/identity$/,
    status: 200,
    handler: () => ({ services: ['dvla', 'mhclg'] }),
  },
  {
    method: 'GET', path: /^\/udp\/v1\/identity\/([^/]+)$/,
    status: 200,
    handler: (m) => ({
      service: m[1],
      userId: USER.userId,
      serviceId: `svc-${m[1]}-001`,
      serviceName: m[1].toUpperCase() + ' Service',
      accessToken: 'eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.mock-access',
      idToken: 'eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.mock-id',
      refreshToken: `refresh-mock-${m[1]}-001`,
    }),
  },
  {
    method: 'POST', path: /^\/udp\/v1\/identity\/([^/]+)$/,
    status: 201,
    handler: (m) => ({ service: m[1], userId: USER.userId, linked: true }),
  },
  {
    method: 'DELETE', path: /^\/udp\/v1\/identity\/([^/]+)$/,
    status: 204,
    handler: () => null,
  },

  // ── /dvla – DVLA APIs ─────────────────────────────────────────────────────

  {
    method: 'GET', path: /^\/dvla\/v1\/driver-summary$/,
    status: 200,
    handler: () => ({
      driverViewResponse: {
        ...DRIVER,
        penaltyPoints: 3,
        disqualification: { disqualified: false },
        eyesight: { standard: true },
        hearing: { standard: true },
        offences: [{ code: 'SP30', points: 3, date: '2023-06-15', expiryDate: '2027-06-15' }],
        previousDrivingLicence: [],
        licenceType: 'Full',
        licenceStatus: 'Full licence',
        countryToWhichExchanged: '',
        entitlements: [{ code: 'B', from: '1993-03-12', to: '2045-03-11', provisional: false, restrictionCodes: [] }],
        testPass: [{ category: 'B', date: '1993-03-11', certNo: 'TC12345678' }],
        endorsements: [{ code: 'SP30', points: 3, offenceDate: '2023-06-15', convictionDate: '2023-09-20' }],
      },
    }),
  },
  {
    method: 'GET', path: /^\/dvla\/v1\/customer-summary$/,
    status: 200,
    handler: () => ({
      customerResponse: {
        customerId: 'cust-001-dvla',
        customerNumber: 'CST-998877',
        identityId: USER.userId,
        recordStatus: 'ACTIVE',
        customerType: 'DRIVER',
        address: DRIVER.address,
        emailAddress: 'sarah.morgan@example.gov.uk',
        phoneNumber: '+44 7700 900123',
        products: ['DRIVING_LICENCE'],
        driversEligibilityResponse: {
          applications: [{
            applicationType: 'RENEWAL',
            isRequired: true,
            ineligibleReason: '',
            availableActions: ['APPLY_ONLINE'],
          }],
        },
        vehicleResponse: [{
          registrationNumber: 'AB23CDX',
          make: 'FORD',
          model: 'FOCUS',
          motStatus: 'VALID',
          fuelType: 'PETROL',
        }],
        hasErrors: false,
      },
    }),
  },
  {
    method: 'GET', path: /^\/dvla\/v1\/driving-licence$/,
    status: 200,
    handler: () => ({
      driver: {
        licenceNumber: DRIVER.dln,
        firstName: DRIVER.firstName,
        lastName: DRIVER.lastName,
        dateOfBirth: DRIVER.dateOfBirth,
        address: DRIVER.address,
        licence: { licenceType: 'Full', licenceStatus: 'Current', statusQualifier: '' },
        licenceType: 'Full',
        licenceStatus: 'Current',
        statusQualifier: '',
        entitlements: [{ code: 'B', from: '1993-03-12', to: '2045-03-11', provisional: false }],
        endorsements: [{ code: 'SP30', points: 3, offenceDate: '2023-06-15' }],
        testPass: [{ category: 'B', date: '1993-03-11', certNo: 'TC12345678' }],
        token: { access: 'mock-access-token' },
        cpc: [],
        holder: { title: 'Ms', sex: 'F' },
      },
    }),
  },
  {
    method: 'GET', path: /^\/dvla\/v1\/vehicle-enquiry\/([^/]+)$/,
    status: 200,
    handler: (m) => ({
      registrationNumber: m[1].toUpperCase(),
      make: 'FORD',
      model: 'FOCUS',
      colour: 'Blue',
      fuelType: 'PETROL',
      engineCapacity: 1596,
      co2Emissions: 129,
      taxStatus: 'Taxed',
      taxDueDate: '2025-01-01',
      motStatus: 'Valid',
      motExpiryDate: '2025-06-30',
      yearOfManufacture: 2016,
      typeApproval: 'M1',
      wheelplan: '2 axle rigid body',
      monthOfFirstRegistration: '2016-04',
    }),
  },
  {
    method: 'GET', path: /^\/dvla\/v1\/share-codes$/,
    status: 200,
    handler: () => ({ shareCodes: [SHARE_CODE] }),
  },
  {
    method: 'POST', path: /^\/dvla\/v1\/share-code$/,
    status: 201,
    handler: () => ({
      shareCodeId: 'sc-' + Date.now().toString(16),
      shareCodeType: 'DRIVING_LICENCE',
      createdAt: new Date().toISOString(),
      expiresAt: new Date(Date.now() + 7 * 24 * 60 * 60 * 1000).toISOString(),
      shareCodeStatus: 'ACTIVE',
    }),
  },
  {
    method: 'POST', path: /^\/dvla\/v1\/share-code\/([^/]+)\/cancel$/,
    status: 200,
    handler: (m) => ({
      id: m[1],
      shareCodeId: m[1],
      shareCodeType: 'DRIVING_LICENCE',
      createdAt: SHARE_CODE.createdAt,
      expiresAt: SHARE_CODE.expiresAt,
      shareCodeStatus: 'CANCELLED',
    }),
  },
  {
    method: 'POST', path: /^\/dvla\/v1\/unlink\/([^/]+)$/,
    status: 200,
    handler: (m) => ({ id: m[1], unlinked: true }),
  },
  {
    method: 'POST', path: /^\/dvla\/v1\/test-notification$/,
    status: 200,
    handler: () => ({ sent: true, timestamp: new Date().toISOString() }),
  },

  // ── /uns – Unified Notification Service ──────────────────────────────────

  {
    method: 'GET', path: /^\/uns\/v1\/notifications$/,
    status: 200,
    handler: () => NOTIFICATIONS,
  },
  {
    method: 'GET', path: /^\/uns\/v1\/notifications\/([^/]+)$/,
    status: 200,
    handler: (m) => ({ ...NOTIFICATIONS[0], notificationId: m[1], NotificationID: m[1] }),
  },
  {
    method: 'DELETE', path: /^\/uns\/v1\/notifications\/([^/]+)$/,
    status: 204,
    handler: () => null,
  },
  {
    method: 'PATCH', path: /^\/uns\/v1\/notifications\/([^/]+)\/status$/,
    status: 200,
    handler: (m, body) => ({ notificationId: m[1], Status: body?.Status ?? 'READ' }),
  },

  // ── /example – FLEX example domain ───────────────────────────────────────
  // More-specific paths appear first to avoid partial matches.

  {
    method: 'GET', path: /^\/example\/v0\/resources\/runtime$/,
    status: 200,
    handler: () => ({ environment: 'development', version: '0.1.0-mock', region: 'eu-west-2', uptime: 3600 }),
  },
  {
    method: 'GET', path: /^\/example\/v0\/resources$/,
    status: 200,
    handler: () => ({
      resources: [
        { name: 'dvla-api', status: 'available', version: 'v1' },
        { name: 'uns-api', status: 'available', version: 'v1' },
        { name: 'local-council-api', status: 'available', version: 'v1' },
      ],
    }),
  },
  {
    method: 'GET', path: /^\/example\/v0\/headers$/,
    status: 200,
    handler: (_, __, req) => ({
      'x-request-id': req.headers['x-request-id'] ?? 'req-' + Date.now().toString(16),
      'x-correlation-id': req.headers['x-correlation-id'] ?? 'corr-' + Date.now().toString(16),
    }),
  },
  {
    method: 'GET', path: /^\/example\/v0\/identity\/([^/]+)$/,
    status: 200,
    handler: (m) => ({
      service: m[1],
      userId: USER.userId,
      serviceId: `svc-${m[1]}-001`,
      serviceName: m[1].toUpperCase() + ' Example Service',
      accessToken: 'eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.mock-access',
      idToken: 'eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.mock-id',
      refreshToken: `refresh-mock-${m[1]}-001`,
    }),
  },
  {
    method: 'POST', path: /^\/example\/v0\/todos\/([^/]+)\/duplicate$/,
    status: 201,
    handler: (m) => ({
      id: 'todo-' + Date.now().toString(16),
      title: 'Duplicated item',
      completed: false,
      priority: 'medium',
      createdAt: new Date().toISOString(),
    }),
  },
  {
    method: 'GET', path: /^\/example\/v0\/todos\/([^/]+)$/,
    status: 200,
    handler: (m) => ({
      id: m[1],
      title: 'Book MOT appointment',
      completed: false,
      priority: 'high',
      createdAt: '2026-07-01T08:00:00Z',
      label: 'Vehicle',
    }),
  },
  {
    method: 'DELETE', path: /^\/example\/v0\/todos\/([^/]+)$/,
    status: 204,
    handler: () => null,
  },
  {
    method: 'GET', path: /^\/example\/v0\/todos$/,
    status: 200,
    handler: () => [
      { id: 'todo-001', title: 'Book MOT appointment', completed: false, priority: 'high', createdAt: '2026-07-01T08:00:00Z' },
      { id: 'todo-002', title: 'Renew driving licence photo', completed: true, priority: 'medium', createdAt: '2026-06-20T10:30:00Z' },
    ],
  },
  {
    method: 'POST', path: /^\/example\/v0\/todos$/,
    status: 201,
    handler: (_, body) => ({
      id: 'todo-' + Date.now().toString(16),
      title: body?.title ?? 'New todo',
      completed: body?.completed ?? false,
      priority: body?.priority ?? 'medium',
      createdAt: new Date().toISOString(),
    }),
  },
  {
    method: 'GET', path: /^\/example\/v0\/users\/notifications$/,
    status: 200,
    handler: () => NOTIFICATIONS,
  },
  {
    method: 'PATCH', path: /^\/example\/v0\/users\/notifications$/,
    status: 200,
    handler: (_, body) => ({
      consentStatus: body?.consentStatus ?? USER.consentStatus,
      pushId: USER.pushId,
      newUserProfileEnabled: true,
    }),
  },
  {
    method: 'PATCH', path: /^\/example\/v0\/notifications$/,
    status: 200,
    handler: (_, body) => ({ consentStatus: body?.consentStatus ?? USER.consentStatus }),
  },

  // ── /local-council – MHCLG Local Council API ─────────────────────────────

  {
    method: 'GET', path: /^\/local-council\/v1\/local-council\/([^/]+)$/,
    status: 200,
    handler: (m) => ({ ...LOCAL_AUTHORITY, id: m[1] }),
  },
  {
    method: 'POST', path: /^\/local-council\/v1\/local-council\/([^/]+)$/,
    status: 200,
    handler: (m, body) => ({
      id: m[1],
      name: body?.name ?? LOCAL_AUTHORITY.name,
      homepage_url: body?.homepage_url ?? LOCAL_AUTHORITY.homepage_url,
      tier: body?.tier ?? LOCAL_AUTHORITY.tier,
      slug: body?.slug ?? LOCAL_AUTHORITY.slug,
      parent: body?.parent ?? null,
    }),
  },
]

// ── Request handling ──────────────────────────────────────────────────────────

function readBody(req) {
  return new Promise((resolve) => {
    let raw = ''
    req.on('data', (chunk) => { raw += chunk })
    req.on('end', () => {
      try { resolve(raw ? JSON.parse(raw) : null) }
      catch { resolve(null) }
    })
  })
}

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, `http://localhost:${PORT}`)
  const pathname = url.pathname
  const body = await readBody(req)

  const matched = ROUTES.find((r) => r.method === req.method && r.path.test(pathname))

  if (!matched) {
    const known = ROUTES.filter((r) => r.path.test(pathname)).map((r) => r.method)
    const status = known.length ? 405 : 404
    res.writeHead(status, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ error: status === 405 ? 'Method not allowed' : 'Not found', path: pathname, method: req.method, allowed: known }))
    console.log(`  ${status}  ${req.method} ${pathname}`)
    return
  }

  const m = pathname.match(matched.path)
  const payload = matched.handler(m, body, req)

  if (matched.status === 204 || payload === null) {
    res.writeHead(204)
    res.end()
    console.log(`  204  ${req.method} ${pathname}`)
    return
  }

  const json = JSON.stringify(payload, null, 2)
  res.writeHead(matched.status, {
    'Content-Type': 'application/json',
    'Content-Length': Buffer.byteLength(json),
    'Access-Control-Allow-Origin': '*',
  })
  res.end(json)
  console.log(`  ${matched.status}  ${req.method} ${pathname}`)
})

server.listen(PORT, '127.0.0.1', () => {
  console.log(`\nMock FLEX API  →  http://localhost:${PORT}`)
  console.log(`${ROUTES.length} routes registered across ${new Set(ROUTES.map((r) => r.path.source.split('/')[1])).size} path prefixes\n`)
})

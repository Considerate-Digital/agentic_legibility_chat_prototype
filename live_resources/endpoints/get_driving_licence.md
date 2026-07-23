---
id: 3f8a2b1c-4d5e-6f7a-8b9c-0d1e2f3a4b5c
name: Get Driving Licence
endpoint: http://localhost:8127/dvla/v1/driving-licence
description: Retrieve the authenticated user's driving licence details including entitlements, endorsements and test passes
department: DVLA
owner: DVLA
method: GET
---
# Get Driving Licence
## Request Structure
driver: object
licenceNumber: string
firstName: string
lastName: string
dateOfBirth: string
address: object
licence: object
licenceType: Provisional | Full
licenceStatus: string
statusQualifier: string
entitlements: array
endorsements: array
testPass: array
token: object
cpc: array
holder: object

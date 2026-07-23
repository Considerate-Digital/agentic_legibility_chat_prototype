---
id: 2b4d6f8a-0c2e-4f6a-8b0d-2e4f6a8b0c2e
name: Get DVLA Driver Summary
endpoint: http://localhost:8127/dvla/v1/driver-summary
description: Retrieve detailed driver summary including licence, entitlements, endorsements and test passes for the authenticated user
department: DVLA
owner: DVLA
method: GET
---
# Get DVLA Driver Summary
## Request Structure
driverViewResponse: object
dln: string
firstName: string
lastName: string
gender: string
dateOfBirth: string
address: object
penaltyPoints: number
disqualification: object
eyesight: object
hearing: object
offences: array
previousDrivingLicence: array
licenceType: string
licenceStatus: string
countryToWhichExchanged: string
entitlements: array
testPass: array
endorsements: array

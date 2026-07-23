---
id: 8b0c2d4e-6f8a-0b2c-4d6e-8f0a2b4c6d8e
name: Get Share Codes
endpoint: http://localhost:8127/dvla/v1/share-codes
description: Retrieve all active and expired driving licence share codes for the authenticated user
department: DVLA
owner: DVLA
method: GET
---
# Get Share Codes
## Request Structure
shareCodes: array
shareCodeId: string
shareCodeType: string
createdAt: string
expiresAt: string
shareCodeStatus: string

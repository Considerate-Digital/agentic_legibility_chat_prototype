---
id: 8f0b2d4e-6a8c-0e2f-4a6c-8e0b2d4f6a8c
name: Get User
endpoint: http://localhost:8127/udp/v1/users/me
description: Retrieve or create the authenticated user's profile including notification consent status and push ID
department: Cabinet Office
owner: GOV.UK One Login
method: GET
---
# Get User
## Request Structure
userId: string
consentStatus: unknown | accepted | denied
pushId: string

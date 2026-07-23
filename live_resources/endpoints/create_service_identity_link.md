---
id: 2d4f6a8b-0c2e-4a6c-8e0f-2b4d6f8a0c2e
name: Create Service Identity Link
endpoint: http://localhost:8127/udp/v1/identity/{service}
description: Link the authenticated user's GOV.UK One Login identity to a specific service using a linking token
department: Cabinet Office
owner: GOV.UK One Login
method: POST
---
# Create Service Identity Link
## Request Structure
service: string
x-linking-token: string

---
id: 9c1e3f5a-7b9d-1f3a-5c7e-9f1b3d5f7a9c
name: Get Service Identity
endpoint: http://localhost:8127/udp/v1/identity/{service}
description: Retrieve the linked identity and tokens for a specific service for the authenticated user
department: Cabinet Office
owner: GOV.UK One Login
method: GET
---
# Get Service Identity
## Request Structure
service: string
userId: string
serviceId: string
serviceName: string
accessToken: string
idToken: string
refreshToken: string

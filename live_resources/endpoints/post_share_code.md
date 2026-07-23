---
id: 1c3e5f7a-9b1c-3e5f-7a9b-1c3e5f7a9b1c
name: Post Share Code
endpoint: http://localhost:8127/dvla/v1/share-code
description: Generate a new driving licence share code for the authenticated user to share their driving record
department: DVLA
owner: DVLA
method: POST
---
# Post Share Code
## Request Structure
shareCodeId: string
shareCodeType: string
createdAt: string
expiresAt: string
shareCodeStatus: string

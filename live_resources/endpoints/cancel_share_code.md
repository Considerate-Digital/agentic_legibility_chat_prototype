---
id: 4d6f8a0b-2c4d-6f8a-0b2c-4d6f8a0b2c4d
name: Cancel Share Code
endpoint: http://localhost:8127/dvla/v1/share-code/{id}/cancel
description: Cancel an existing driving licence share code so it can no longer be used to view the user's record
department: DVLA
owner: DVLA
method: POST
---
# Cancel Share Code
## Request Structure
id: string
shareCodeId: string
shareCodeType: string
createdAt: string
expiresAt: string
shareCodeStatus: string

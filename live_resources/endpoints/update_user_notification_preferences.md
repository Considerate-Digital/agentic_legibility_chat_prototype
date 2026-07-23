---
id: 4b6d8f0a-2c4e-6a8c-0e2f-4b6d8f0a2c4e
name: Update User Notification Preferences
endpoint: http://localhost:8127/udp/v1/users/me/notifications
description: Update the authenticated user's notification consent status and return the updated preferences including push ID
department: Cabinet Office
owner: GOV.UK One Login
method: PATCH
---
# Update User Notification Preferences
## Request Structure
consentStatus: unknown | accepted | denied
pushId: string

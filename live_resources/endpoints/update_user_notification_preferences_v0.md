---
id: 2a4c6e8f-0b2d-4c6e-8f0b-2d4f6a8c0e2f
name: Update User Notification Preferences
endpoint: http://localhost:8127/example/v0/users/notifications
description: Update the authenticated user's notification consent preferences and return updated status with feature flags in the FLEX example domain
department: Cabinet Office
owner: GOV.UK One Login
method: PATCH
---
# Update User Notification Preferences
## Request Structure
consentStatus: unknown | accepted | denied
pushId: string
newUserProfileEnabled: boolean

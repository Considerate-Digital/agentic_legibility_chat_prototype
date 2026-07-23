---
id: 6f8b0d2e-4a6c-8e0f-2b4d-6f8b0d2e4a6c
name: Patch Notification Status
endpoint: http://localhost:8127/uns/v1/notifications/{notificationId}/status
description: Update the read status of a specific notification for the authenticated user
department: Cabinet Office
owner: GOV.UK One Login
method: PATCH
---
# Patch Notification Status
## Request Structure
notificationId: string
Status: RECEIVED | READ | MARKED_AS_UNREAD

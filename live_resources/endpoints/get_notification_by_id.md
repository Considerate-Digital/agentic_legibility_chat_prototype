---
id: 0d2f4a6b-8c0e-2f4a-6b8d-0f2a4c6e8b0d
name: Get Notification By ID
endpoint: http://localhost:8127/uns/v1/notifications/{notificationId}
description: Retrieve a single notification by its ID for the authenticated user
department: Cabinet Office
owner: GOV.UK One Login
method: GET
---
# Get Notification By ID
## Request Structure
notificationId: string
NotificationID: string
NotificationTitle: string
NotificationBody: string
MessageTitle: string
MessageBody: string
DispatchedDateTime: string
Status: RECEIVED | READ | MARKED_AS_UNREAD | HIDDEN

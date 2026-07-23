---
id: 7c9e1f3a-5b7d-9f1a-3c5e-7b9d1f3a5c7e
name: Get Notifications
endpoint: http://localhost:8127/uns/v1/notifications
description: Retrieve all notifications for the authenticated user including read status and message content
department: Cabinet Office
owner: GOV.UK One Login
method: GET
---
# Get Notifications
## Request Structure
NotificationID: string
NotificationTitle: string
NotificationBody: string
MessageTitle: string
MessageBody: string
DispatchedDateTime: string
Status: RECEIVED | READ | MARKED_AS_UNREAD | HIDDEN

---
id: 9f1b3d5f-7a9c-1f3b-5d7f-9a1c3e5f7a9c
name: Get User Notifications
endpoint: http://localhost:8127/example/v0/users/notifications
description: Retrieve all notifications for the authenticated user in the FLEX example domain
department: Cabinet Office
owner: GOV.UK One Login
method: GET
---
# Get User Notifications
## Request Structure
NotificationID: string
NotificationTitle: string
NotificationBody: string
MessageTitle: string
MessageBody: string
DispatchedDateTime: string
Status: RECEIVED | READ | MARKED_AS_UNREAD | HIDDEN

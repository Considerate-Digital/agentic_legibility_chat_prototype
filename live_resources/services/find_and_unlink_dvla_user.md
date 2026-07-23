---
id: a475004d-2965-4433-97a0-feb812ce19d1
name: Find and unlink DVLA user
description: Locates and separates a user from their DVLA account.
departments: Cabinet Office, DVLA
owner: alex@gov
status: publish
type: service
policy_ref: 
source_services: 
---
1. 77c152be-efae-4f4d-99ee-d28e3a7fb0cc, Get User, Cabinet Office, optional
2. 9802118e-fbb3-4830-a5b2-b5e04ec6e150, Get DVLA Customer Summary, DVLA, optional
3. bd21cd21-5300-42f4-9242-28e9a88305f1, Get DVLA Driver Summary, DVLA, optional
4. c5267c41-9d96-470f-aeeb-870892ff1aa5, Unlink DVLA User, DVLA, optional
5. b2eebe17-0671-4c8f-91a5-05a14110f9a6, Get Notifications, Cabinet Office, optional
6. 7f6478fc-c1ab-46c8-9db7-5d8aa1e9e67d, Delete Notification, Cabinet Office, optional

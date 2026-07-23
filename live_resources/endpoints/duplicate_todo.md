---
id: 1e3f5a7c-9d1f-3a5c-7e9f-1b3d5f7a9c1e
name: Duplicate Todo
endpoint: http://localhost:8127/example/v0/todos/{id}/duplicate
description: Create a copy of an existing todo item returning the newly created duplicate
department: Cabinet Office
owner: GOV.UK One Login
method: POST
---
# Duplicate Todo
## Request Structure
id: string
title: string
completed: boolean
priority: low | medium | high
createdAt: string

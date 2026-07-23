---
id: 5a7c9e1f-3b5d-7a9c-1e3f-5b7d9a1c3e5f
name: Get Todo
endpoint: http://localhost:8127/example/v0/todos/{id}
description: Retrieve a single todo item by its ID
department: Cabinet Office
owner: GOV.UK One Login
method: GET
---
# Get Todo
## Request Structure
id: string
title: string
completed: boolean
priority: low | medium | high
createdAt: string
label: string

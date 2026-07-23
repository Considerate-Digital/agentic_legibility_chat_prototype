---
id: 9a0c2e4f-6b8d-0f2a-4c6e-8a0c2e4f6b8d
name: List Todos
endpoint: http://localhost:8127/example/v0/todos
description: Retrieve a paginated list of todos with optional filtering by completion status and priority
department: Cabinet Office
owner: GOV.UK One Login
method: GET
---
# List Todos
## Request Structure
completed: boolean
priority: low | medium | high
limit: number
page: number
id: string
title: string
createdAt: string

---
id: 3a5c7e9f-1b3d-5a7c-9e1f-3b5d7a9c1e3f
name: Upsert Local Authority
endpoint: http://localhost:8127/local-council/v1/local-council/{id}
description: Create or update a local authority record with name, tier classification, homepage URL and optional parent authority
department: MHCLG
owner: MHCLG
method: POST
---
# Upsert Local Authority
## Request Structure
id: string
name: string
homepage_url: string
tier: county | district | unitary | metropolitan | london_borough
slug: string
parent: object

---
id: 0f2a4c6e-8b0d-2f4a-6c8e-0b2d4f6a8c0e
name: Get Local Authority
endpoint: http://localhost:8127/local-council/v1/local-council/{id}
description: Retrieve details for a specific local authority including name, tier, homepage URL and parent authority
department: MHCLG
owner: MHCLG
method: GET
---
# Get Local Authority
## Request Structure
id: string
name: string
homepage_url: string
tier: county | district | unitary | metropolitan | london_borough
slug: string
parent: object

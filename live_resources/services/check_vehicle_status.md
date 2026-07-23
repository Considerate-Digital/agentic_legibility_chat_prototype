---
id: dd7ff1f5-d142-4d1e-86ad-d62bd2b7be6f
name: Check vehicle status
description: Investigate the user's vehicle status
departments: DVLA
owner: alex@gov
status: publish
type: service
policy_ref: 
source_services: 
---
1. 2b4d6f8a-0c2e-4f6a-8b0d-2e4f6a8b0c2e, Get DVLA Driver Summary, DVLA, optional
2. 5e7f9a1b-3c5d-7e9f-1a3b-5c7d9e1f3a5b, Get Vehicle Enquiry, DVLA, required
3. 7e9a1b3c-5d7e-9a1b-3c5d-7e9a1b3c5d7e, Unlink DVLA User, DVLA, optional

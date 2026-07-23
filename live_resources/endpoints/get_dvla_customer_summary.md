---
id: 9a1b3c5d-7e9f-1a3b-5c7d-9e1f3a5b7c9d
name: Get DVLA Customer Summary
endpoint: http://localhost:8127/dvla/v1/customer-summary
description: Retrieve a full summary of the authenticated customer's DVLA record including vehicles, eligibility and application tasks
department: DVLA
owner: DVLA
method: GET
---
# Get DVLA Customer Summary
## Request Structure
customerResponse: object
customerId: string
customerNumber: string
identityId: string
recordStatus: string
customerType: string
address: object
emailAddress: string
phoneNumber: string
products: array
driversEligibilityResponse: object
applications: array
applicationType: string
isRequired: boolean
ineligibleReason: string
availableActions: array
vehicleResponse: array
registrationNumber: string
make: string
model: string
motStatus: string
fuelType: string
hasErrors: boolean

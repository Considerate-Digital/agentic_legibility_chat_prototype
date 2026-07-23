---
id: 9cfd72bb-9709-48bc-ae99-be6bcfa5e56b
name: Change driving licence address
description: Change a user's driving licence address using either a postcode search or by entering the address manually.
departments: AL
owner: alex@gov
status: publish
type: service
policy_ref: 
source_services: 
---
1. 32de4924-5ad5-4d5b-935a-40dd6e226dff, choose_address_entry_method, AL, required
The user must follow endpoint path two or three.
2. 23a0fcfa-7d9a-4280-a791-5b8a302e9f22, find_address_by_postcode, AL, optional
3. 710c67a7-99f1-400d-bf0c-b9d8eb24b265, enter_address_manually, AL, optional
4. e6929989-ff87-4787-9de0-7645f1d3037d, confirm_new_address, AL required
At the end of the interaction confirm if the new address has been updated sucessfully.

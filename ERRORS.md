# Error Codes Mapping

When interacting with the Grant Stream smart contracts, developers might encounter generic numerical error codes (e.g., `Error(7)`). This table maps these numerical codes to human-readable reasons to help with debugging.

| Error Code | Human-Readable Reason   | Description                                                                       |
| ---------- | ----------------------- | --------------------------------------------------------------------------------- |
| `1`        | Not Initialized         | Contract has not been initialized yet.                                             |
| `2`        | Already Initialized      | Contract has already been initialized.                                              |
| `3`        | Not Authorized          | The caller does not have the required permissions.                                |
| `4`        | Grant Not Found         | The specified grant ID does not exist in storage.                                 |
| `5`        | Grant Already Exists     | A grant with the specified ID already exists.                                    |
| `6`        | Invalid Rate           | The specified flow rate is invalid (negative or zero).                             |
| `7`        | Invalid Amount         | The specified amount is invalid (e.g., exceeds remaining balance or total grant). |
| `8`        | Invalid State          | The operation is not valid in the current grant state.                             |
| `9`        | Math Overflow          | Arithmetic operation would cause an overflow.                                      |
| `10`       | Insufficient Reserve   | Not enough reserve XLM to complete the operation.                                  |
| `11`       | Rescue Would Violate Allocated | Rescue operation would violate allocated funds.                           |
| `12`       | Grantee Mismatch       | The caller is not the grant recipient.                                            |
| `13`       | Grant Not Inactive     | Grant has not been inactive long enough for the operation.                         |
| `14`       | NFT Already Minted    | A completion certificate NFT has already been minted for this grant.               |
| `15`       | NFT Max Supply Reached | Maximum NFT supply for completion certificates has been reached.                   |
| `16`       | NFT Not Found         | The specified NFT token ID does not exist.                                         |

_Note: If you encounter an error code not listed here, please verify the contract source code or Soroban SDK standard errors._

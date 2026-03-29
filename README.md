# Grant Stream Contracts

Smart contracts for managing grant streams with milestone completion proof hashing.

## Overview

This implementation addresses Issue #203: Support for Milestone Completion Proof Hashing. The contract allows grantees to submit SHA-256 hashes of their deliverables as immutable proof of milestone completion, creating a transparent audit trail for institutional grants.

## Features

### Core Functionality
- **Grant Creation**: Create grant streams with multiple milestones
- **Milestone Management**: Define milestones with deadlines and amounts
- **Proof Submission**: Submit SHA-256 hashes of deliverables (PDFs, GitHub releases, etc.)
- **Immutable Audit Trail**: Store completion time, proof hash, and metadata on-chain
- **Access Control**: Role-based permissions for creators and grantees

### Hashing Utilities
- SHA-256 hashing for various data types
- File metadata hashing (filename, content, type)
- GitHub release metadata hashing
- Concatenated data hashing
- Proof verification functions

## Key Components

### GrantStream.sol
Main contract implementing the grant stream functionality with proof hashing.

### HashUtils.sol
Library providing SHA-256 hashing utilities for different use cases.

## Usage Examples

### Creating a Grant with Milestones

```solidity
// Create a grant
uint256 grantId = grantStream.createGrant(
    granteeAddress,
    1000 ether,
    "Research Grant",
    "Funding for blockchain research"
);

// Create milestones
uint256 milestone1 = grantStream.createMilestone(
    grantId,
    "Literature Review",
    "Complete comprehensive literature review",
    250 ether,
    deadline1
);

uint256 milestone2 = grantStream.createMilestone(
    grantId,
    "Prototype Development",
    "Build working prototype",
    500 ether,
    deadline2
);
```

### Submitting Proof for Milestone Completion

```solidity
// For a PDF report
bytes memory pdfContent = "..."; // PDF file content
bytes32 proofHash = sha256(pdfContent);

grantStream.submitProof(
    milestoneId,
    proofHash,
    "Q1 Research Report PDF"
);

// For a GitHub release
bytes32 releaseHash = HashUtils.createReleaseHash(
    "https://github.com/user/repo",
    "v1.0.0",
    "abc123def456",
    "Initial release with all features"
);

grantStream.submitProof(
    milestoneId,
    releaseHash,
    "GitHub Release v1.0.0"
);
```

### Verifying Proof

```solidity
// Verify that submitted data matches the stored hash
bool isValid = grantStream.verifyProof(storedHash, originalData);

// Get immutable audit trail
(uint256 completionTime, bytes32 proofHash, string memory metadata) = 
    grantStream.getMilestoneAuditTrail(milestoneId);
```

## Installation

```bash
# Clone the repository
git clone https://github.com/lifewithbigdamz/Grant-Stream-Contracts.git
cd Grant-Stream-Contracts

# Install dependencies
forge install

# Build contracts
forge build

# Run tests
forge test
```

## Testing

The contract includes comprehensive tests covering:

- Valid proof submission scenarios
- GitHub release and PDF file hashing
- Access control and permission checks
- Deadline enforcement
- Hash verification
- Audit trail functionality
- Multiple milestone management

Run tests with:
```bash
forge test
```

Run test coverage:
```bash
forge coverage
```

## Security Considerations

- **Reentrancy Protection**: Uses OpenZeppelin's ReentrancyGuard
- **Access Control**: Role-based permissions for creators and grantees
- **Input Validation**: Validates all inputs including proof hashes and deadlines
- **Immutable Storage**: Once submitted, proofs cannot be modified
- **Gas Optimization**: Efficient storage patterns for milestone data

## Audit Trail Features

The immutable audit trail provides:

1. **Completion Timestamp**: Exact time when proof was submitted
2. **Proof Hash**: SHA-256 hash of the deliverable
3. **Metadata**: Optional descriptive information about the proof
4. **Grantee Address**: Address that submitted the proof
5. **Milestone Details**: Full context of the completed milestone

This creates a verifiable, tamper-proof record suitable for institutional audit requirements.

## Integration

The contracts are designed to integrate with:

- **IPFS/Filecoin**: For storing actual deliverable files
- **GitHub API**: For automatic release hash generation
- **Document Management Systems**: For PDF report processing
- **Audit Platforms**: For compliance verification

## License

MIT License - see LICENSE file for details.

## Contributing

Please follow the contribution guidelines and ensure all tests pass before submitting pull requests.

## Issues

For issues related to milestone completion proof hashing, please reference Issue #203.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

/**
 * @title HashUtils
 * @dev Library for SHA-256 hashing operations
 * @notice Provides utilities for creating and verifying SHA-256 hashes
 */
library HashUtils {
    
    /**
     * @dev Computes SHA-256 hash of the given data
     * @param _data The data to hash
     * @return The SHA-256 hash
     */
    function sha256(bytes memory _data) internal pure returns (bytes32) {
        return sha256(_data);
    }

    /**
     * @dev Computes SHA-256 hash of a string
     * @param _data The string to hash
     * @return The SHA-256 hash
     */
    function sha256(string memory _data) internal pure returns (bytes32) {
        return sha256(bytes(_data));
    }

    /**
     * @dev Computes SHA-256 hash of concatenated data
     * @param _data1 First piece of data
     * @param _data2 Second piece of data
     * @return The SHA-256 hash of concatenated data
     */
    function sha256Concat(bytes memory _data1, bytes memory _data2) internal pure returns (bytes32) {
        return sha256(abi.encodePacked(_data1, _data2));
    }

    /**
     * @dev Computes SHA-256 hash of multiple data pieces
     * @param _data Array of data pieces to hash
     * @return The SHA-256 hash of all concatenated data
     */
    function sha256Multiple(bytes[] memory _data) internal pure returns (bytes32) {
        return sha256(abi.encodePacked(_data));
    }

    /**
     * @dev Verifies that data matches a given hash
     * @param _hash The expected hash
     * @param _data The data to verify
     * @return True if the data hashes to the expected value
     */
    function verifyHash(bytes32 _hash, bytes memory _data) internal pure returns (bool) {
        return sha256(_data) == _hash;
    }

    /**
     * @dev Verifies that string data matches a given hash
     * @param _hash The expected hash
     * @param _data The string data to verify
     * @return True if the data hashes to the expected value
     */
    function verifyHash(bytes32 _hash, string memory _data) internal pure returns (bool) {
        return sha256(_data) == _hash;
    }

    /**
     * @dev Creates a hash for file metadata (useful for PDF reports, GitHub releases, etc.)
     * @param _fileName Name of the file
     * @param _fileContent Content of the file
     * @param _fileType Type of the file (e.g., "PDF", "ZIP", "SOURCE_CODE")
     * @return The SHA-256 hash combining all metadata
     */
    function createFileHash(
        string memory _fileName,
        bytes memory _fileContent,
        string memory _fileType
    ) internal pure returns (bytes32) {
        return sha256(abi.encodePacked(_fileName, _fileContent, _fileType));
    }

    /**
     * @dev Creates a hash for GitHub release metadata
     * @param _repoUrl Repository URL
     * @param _releaseTag Release tag
     * @param _commitHash Commit hash
     * @param _releaseNotes Release notes
     * @return The SHA-256 hash combining all release metadata
     */
    function createReleaseHash(
        string memory _repoUrl,
        string memory _releaseTag,
        string memory _commitHash,
        string memory _releaseNotes
    ) internal pure returns (bytes32) {
        return sha256(abi.encodePacked(_repoUrl, _releaseTag, _commitHash, _releaseNotes));
    }

    /**
     * @dev Creates a hash for milestone deliverable with timestamp
     * @param _deliverableHash Hash of the actual deliverable
     * @param _timestamp Submission timestamp
     * @param _grantee Address of the grantee
     * @return The SHA-256 hash combining deliverable info with metadata
     */
    function createMilestoneHash(
        bytes32 _deliverableHash,
        uint256 _timestamp,
        address _grantee
    ) internal pure returns (bytes32) {
        return sha256(abi.encodePacked(_deliverableHash, _timestamp, _grantee));
    }
}

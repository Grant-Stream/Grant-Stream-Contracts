// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/**
 * @title IGrantStreamLogic
 * @notice Interface every logic implementation must satisfy.
 *         The proxy delegates calls here; immutable terms (funder, recipient,
 *         totalAmount) are stored in the proxy and NEVER touched by logic.
 */
interface IGrantStreamLogic {
    /// @notice Called once when a new logic version is activated.
    ///         Must not alter immutable terms stored in the proxy.
    function initialize(address sustainabilityFund) external;

    function claim(uint256 grantId, uint256 amount) external;

    function topUp(uint256 grantId) external payable;

    function closeGrant(uint256 grantId) external;
}

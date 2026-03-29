// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/**
 * @title SustainabilityFund
 * @notice The JerryIdoko Developer Treasury.
 *         Receives the 0.01% sustainability tax from GrantStream once a grant
 *         surpasses $100,000 in cumulative volume
 *         (Final_Protocol_Sustainability_Fund_Transfer).
 *
 *         Only the designated treasury address can withdraw accumulated funds.
 *         The treasury address is set at deployment and can be updated by the owner.
 */
contract SustainabilityFund is Ownable, ReentrancyGuard {
    // ─── State ────────────────────────────────────────────────────────────────

    address public treasury;
    uint256 public totalReceived;

    // ─── Events ───────────────────────────────────────────────────────────────

    event TaxDeposited(address indexed from, uint256 amount, uint256 newTotal);
    event TreasuryWithdrawal(address indexed treasury, uint256 amount);
    event TreasuryUpdated(address indexed oldTreasury, address indexed newTreasury);

    // ─── Constructor ──────────────────────────────────────────────────────────

    /**
     * @param _treasury  The JerryIdoko Developer Treasury address.
     */
    constructor(address _treasury) Ownable(msg.sender) {
        require(_treasury != address(0), "SustainabilityFund: zero treasury");
        treasury = _treasury;
    }

    // ─── External ─────────────────────────────────────────────────────────────

    /**
     * @notice Called by GrantStream to deposit the sustainability tax.
     */
    function deposit() external payable {
        require(msg.value > 0, "SustainabilityFund: zero deposit");
        totalReceived += msg.value;
        emit TaxDeposited(msg.sender, msg.value, totalReceived);
    }

    /**
     * @notice Withdraw accumulated funds to the treasury address.
     * @param amount Amount to withdraw (0 = withdraw all).
     */
    function withdraw(uint256 amount) external nonReentrant {
        require(msg.sender == treasury, "SustainabilityFund: not treasury");

        uint256 toSend = amount == 0 ? address(this).balance : amount;
        require(toSend <= address(this).balance, "SustainabilityFund: insufficient balance");

        (bool ok, ) = treasury.call{value: toSend}("");
        require(ok, "SustainabilityFund: withdrawal failed");

        emit TreasuryWithdrawal(treasury, toSend);
    }

    /**
     * @notice Owner can update the treasury address (e.g. key rotation).
     */
    function setTreasury(address _newTreasury) external onlyOwner {
        require(_newTreasury != address(0), "SustainabilityFund: zero address");
        emit TreasuryUpdated(treasury, _newTreasury);
        treasury = _newTreasury;
    }

    /// @notice Returns current contract balance.
    function balance() external view returns (uint256) {
        return address(this).balance;
    }

    receive() external payable {
        totalReceived += msg.value;
        emit TaxDeposited(msg.sender, msg.value, totalReceived);
    }
}

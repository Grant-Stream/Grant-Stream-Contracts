// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import "./GrantStreamProxy.sol";

/**
 * @title DAOUpgradeGovernance
 * @notice Minimal DAO that lets token-weighted members vote to rotate the
 *         GrantStreamProxy logic implementation.
 *
 * Flow:
 *  1. Any member proposes a new logic address via propose().
 *  2. Members cast yes/no votes during the VOTING_PERIOD.
 *  3. After the period, anyone calls execute() — if quorum and majority are
 *     met the proxy's upgradeLogic() is invoked.
 *
 * Voting power is a simple 1-address-1-vote model; replace with an ERC-20
 * snapshot if token-weighted governance is needed.
 */
contract DAOUpgradeGovernance is ReentrancyGuard {

    // ─── Config ───────────────────────────────────────────────────────────────

    uint256 public constant VOTING_PERIOD  = 3 days;
    uint256 public constant QUORUM         = 3;   // minimum yes votes required

    GrantStreamProxy public immutable proxy;

    // ─── Member Registry ──────────────────────────────────────────────────────

    mapping(address => bool) public members;
    uint256 public memberCount;

    address public admin;

    // ─── Proposal ─────────────────────────────────────────────────────────────

    struct Proposal {
        address   newImpl;
        uint256[] sampleGrantIds;   // forwarded to proxy.upgradeLogic()
        uint256   votesFor;
        uint256   votesAgainst;
        uint256   deadline;
        bool      executed;
        bool      exists;
    }

    uint256 public nextProposalId;
    mapping(uint256 => Proposal)                     public proposals;
    mapping(uint256 => mapping(address => bool))     public hasVoted;

    // ─── Events ───────────────────────────────────────────────────────────────

    event MemberAdded(address indexed member);
    event MemberRemoved(address indexed member);
    event ProposalCreated(uint256 indexed proposalId, address indexed newImpl, address indexed proposer);
    event Voted(uint256 indexed proposalId, address indexed voter, bool support);
    event ProposalExecuted(uint256 indexed proposalId, address indexed newImpl);
    event ProposalDefeated(uint256 indexed proposalId);

    // ─── Constructor ──────────────────────────────────────────────────────────

    constructor(address _proxy) {
        require(_proxy != address(0), "DAO: zero proxy");
        proxy = GrantStreamProxy(_proxy);
        admin = msg.sender;
        _addMember(msg.sender);
    }

    // ─── Member Management (admin-only) ───────────────────────────────────────

    function addMember(address member) external {
        require(msg.sender == admin, "DAO: not admin");
        _addMember(member);
    }

    function removeMember(address member) external {
        require(msg.sender == admin, "DAO: not admin");
        require(members[member], "DAO: not a member");
        members[member] = false;
        memberCount--;
        emit MemberRemoved(member);
    }

    // ─── Governance ───────────────────────────────────────────────────────────

    /**
     * @notice Propose a logic rotation.
     * @param newImpl        Address of the candidate logic contract.
     * @param sampleGrantIds Grant IDs to spot-check in the proxy (can be empty).
     */
    function propose(
        address newImpl,
        uint256[] calldata sampleGrantIds
    ) external returns (uint256 proposalId) {
        require(members[msg.sender], "DAO: not a member");
        require(newImpl != address(0), "DAO: zero impl");

        proposalId = nextProposalId++;
        proposals[proposalId] = Proposal({
            newImpl:        newImpl,
            sampleGrantIds: sampleGrantIds,
            votesFor:       0,
            votesAgainst:   0,
            deadline:       block.timestamp + VOTING_PERIOD,
            executed:       false,
            exists:         true
        });

        emit ProposalCreated(proposalId, newImpl, msg.sender);
    }

    /**
     * @notice Cast a vote on an open proposal.
     * @param proposalId Proposal to vote on.
     * @param support    True = yes, false = no.
     */
    function vote(uint256 proposalId, bool support) external {
        require(members[msg.sender],          "DAO: not a member");
        Proposal storage p = proposals[proposalId];
        require(p.exists,                     "DAO: unknown proposal");
        require(block.timestamp < p.deadline, "DAO: voting closed");
        require(!hasVoted[proposalId][msg.sender], "DAO: already voted");

        hasVoted[proposalId][msg.sender] = true;
        if (support) { p.votesFor++;     }
        else         { p.votesAgainst++; }

        emit Voted(proposalId, msg.sender, support);
    }

    /**
     * @notice Execute a passed proposal after the voting period ends.
     *         Calls proxy.upgradeLogic() which enforces the immutable-terms
     *         invariant before accepting the new implementation.
     */
    function execute(uint256 proposalId) external nonReentrant {
        Proposal storage p = proposals[proposalId];
        require(p.exists,                      "DAO: unknown proposal");
        require(block.timestamp >= p.deadline, "DAO: voting still open");
        require(!p.executed,                   "DAO: already executed");

        p.executed = true;

        if (p.votesFor >= QUORUM && p.votesFor > p.votesAgainst) {
            proxy.upgradeLogic(p.newImpl, p.sampleGrantIds);
            emit ProposalExecuted(proposalId, p.newImpl);
        } else {
            emit ProposalDefeated(proposalId);
        }
    }

    // ─── View ─────────────────────────────────────────────────────────────────

    function getProposal(uint256 proposalId) external view returns (Proposal memory) {
        return proposals[proposalId];
    }

    // ─── Internal ─────────────────────────────────────────────────────────────

    function _addMember(address member) internal {
        require(!members[member], "DAO: already a member");
        members[member] = true;
        memberCount++;
        emit MemberAdded(member);
    }
}

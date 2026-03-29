const { expect } = require("chai");
const { ethers } = require("hardhat");

// ─── Helpers ──────────────────────────────────────────────────────────────────

const DUMMY_PROOF = ethers.keccak256(ethers.toUtf8Bytes("stellar-aid-assist-proof-v1"));

// ─── ZKKYCVerifier unit tests ─────────────────────────────────────────────────

describe("ZKKYCVerifier", function () {
    let verifierContract;
    let owner, verifierEOA, grantee, other;

    beforeEach(async () => {
        [owner, verifierEOA, grantee, other] = await ethers.getSigners();

        const ZKV = await ethers.getContractFactory("ZKKYCVerifier");
        verifierContract = await ZKV.deploy(verifierEOA.address);
    });

    // ── Deployment ────────────────────────────────────────────────────────────

    it("sets the verifier address on deployment", async () => {
        expect(await verifierContract.verifier()).to.equal(verifierEOA.address);
    });

    it("reverts deployment when verifier is zero address", async () => {
        const ZKV = await ethers.getContractFactory("ZKKYCVerifier");
        await expect(ZKV.deploy(ethers.ZeroAddress))
            .to.be.revertedWith("ZKKYCVerifier: zero verifier");
    });

    // ── isVerified ────────────────────────────────────────────────────────────

    it("returns false for an address that has not been verified", async () => {
        expect(await verifierContract.isVerified(grantee.address)).to.be.false;
    });

    it("stores no personal data — only the proof hash is accessible", async () => {
        await verifierContract.connect(verifierEOA).verifyAddress(grantee.address, DUMMY_PROOF);
        // Only the hash is stored; no name, passport, or PII
        expect(await verifierContract.proofHashes(grantee.address)).to.equal(DUMMY_PROOF);
        expect(await verifierContract.isVerified(grantee.address)).to.be.true;
    });

    // ── verifyAddress ─────────────────────────────────────────────────────────

    it("verifier can verify an address with a proof hash", async () => {
        await expect(
            verifierContract.connect(verifierEOA).verifyAddress(grantee.address, DUMMY_PROOF)
        )
            .to.emit(verifierContract, "AddressVerified")
            .withArgs(grantee.address, DUMMY_PROOF);

        expect(await verifierContract.isVerified(grantee.address)).to.be.true;
    });

    it("non-verifier cannot verify an address", async () => {
        await expect(
            verifierContract.connect(other).verifyAddress(grantee.address, DUMMY_PROOF)
        ).to.be.revertedWith("ZKKYCVerifier: caller is not verifier");
    });

    it("reverts when verifying the zero address", async () => {
        await expect(
            verifierContract.connect(verifierEOA).verifyAddress(ethers.ZeroAddress, DUMMY_PROOF)
        ).to.be.revertedWith("ZKKYCVerifier: zero account");
    });

    it("reverts when proof hash is zero (reserved sentinel)", async () => {
        await expect(
            verifierContract.connect(verifierEOA).verifyAddress(grantee.address, ethers.ZeroHash)
        ).to.be.revertedWith("ZKKYCVerifier: zero proof hash");
    });

    it("verifier can update a proof hash for an already-verified address", async () => {
        await verifierContract.connect(verifierEOA).verifyAddress(grantee.address, DUMMY_PROOF);
        const newProof = ethers.keccak256(ethers.toUtf8Bytes("updated-proof-v2"));
        await verifierContract.connect(verifierEOA).verifyAddress(grantee.address, newProof);
        expect(await verifierContract.proofHashes(grantee.address)).to.equal(newProof);
    });

    // ── revokeVerification ────────────────────────────────────────────────────

    it("verifier can revoke a verified address", async () => {
        await verifierContract.connect(verifierEOA).verifyAddress(grantee.address, DUMMY_PROOF);

        await expect(verifierContract.connect(verifierEOA).revokeVerification(grantee.address))
            .to.emit(verifierContract, "VerificationRevoked")
            .withArgs(grantee.address);

        expect(await verifierContract.isVerified(grantee.address)).to.be.false;
        expect(await verifierContract.proofHashes(grantee.address)).to.equal(ethers.ZeroHash);
    });

    it("reverts revoking an address that is not verified", async () => {
        await expect(
            verifierContract.connect(verifierEOA).revokeVerification(grantee.address)
        ).to.be.revertedWith("ZKKYCVerifier: not verified");
    });

    it("non-verifier cannot revoke verification", async () => {
        await verifierContract.connect(verifierEOA).verifyAddress(grantee.address, DUMMY_PROOF);
        await expect(
            verifierContract.connect(other).revokeVerification(grantee.address)
        ).to.be.revertedWith("ZKKYCVerifier: caller is not verifier");
    });

    // ── setVerifier ───────────────────────────────────────────────────────────

    it("owner can rotate the verifier address", async () => {
        await expect(verifierContract.connect(owner).setVerifier(other.address))
            .to.emit(verifierContract, "VerifierUpdated")
            .withArgs(verifierEOA.address, other.address);

        expect(await verifierContract.verifier()).to.equal(other.address);
    });

    it("non-owner cannot rotate the verifier address", async () => {
        await expect(
            verifierContract.connect(other).setVerifier(other.address)
        ).to.be.revertedWithCustomError(verifierContract, "OwnableUnauthorizedAccount");
    });

    it("reverts setting verifier to zero address", async () => {
        await expect(
            verifierContract.connect(owner).setVerifier(ethers.ZeroAddress)
        ).to.be.revertedWith("ZKKYCVerifier: zero verifier");
    });
});

// ─── GrantStream + ZKKYCVerifier integration tests ────────────────────────────

describe("GrantStream — ZK-KYC integration", function () {
    let grantStream, fund, verifierContract;
    let owner, verifierEOA, funder, recipient, unverified, treasury;

    beforeEach(async () => {
        [owner, verifierEOA, funder, recipient, unverified, treasury] = await ethers.getSigners();

        const Fund = await ethers.getContractFactory("SustainabilityFund");
        fund = await Fund.deploy(treasury.address);

        const GS = await ethers.getContractFactory("GrantStream");
        grantStream = await GS.deploy(await fund.getAddress());

        const ZKV = await ethers.getContractFactory("ZKKYCVerifier");
        verifierContract = await ZKV.deploy(verifierEOA.address);

        // Wire the verifier into GrantStream and enable KYC requirement
        await grantStream.connect(owner).setZKVerifier(await verifierContract.getAddress());
        await grantStream.connect(owner).setKYCRequired(true);

        // KYC-verify the recipient
        await verifierContract.connect(verifierEOA).verifyAddress(recipient.address, DUMMY_PROOF);
    });

    // ── setZKVerifier / setKYCRequired ────────────────────────────────────────

    it("emits ZKVerifierSet when verifier is configured", async () => {
        const GS = await ethers.getContractFactory("GrantStream");
        const fresh = await GS.deploy(await fund.getAddress());
        await expect(fresh.connect(owner).setZKVerifier(await verifierContract.getAddress()))
            .to.emit(fresh, "ZKVerifierSet")
            .withArgs(await verifierContract.getAddress());
    });

    it("emits KYCRequirementChanged when toggled", async () => {
        await expect(grantStream.connect(owner).setKYCRequired(false))
            .to.emit(grantStream, "KYCRequirementChanged")
            .withArgs(false);
    });

    it("reverts enabling KYC when zkVerifier is not set", async () => {
        const GS = await ethers.getContractFactory("GrantStream");
        const fresh = await GS.deploy(await fund.getAddress());
        await expect(fresh.connect(owner).setKYCRequired(true))
            .to.be.revertedWith("GrantStream: zkVerifier not set");
    });

    it("non-owner cannot set zkVerifier", async () => {
        await expect(
            grantStream.connect(funder).setZKVerifier(await verifierContract.getAddress())
        ).to.be.revertedWithCustomError(grantStream, "OwnableUnauthorizedAccount");
    });

    it("non-owner cannot change KYC requirement", async () => {
        await expect(
            grantStream.connect(funder).setKYCRequired(false)
        ).to.be.revertedWithCustomError(grantStream, "OwnableUnauthorizedAccount");
    });

    // ── createGrant with KYC ──────────────────────────────────────────────────

    it("allows creating a grant for a KYC-verified recipient", async () => {
        const amount = ethers.parseEther("1");
        const tx = await grantStream.connect(funder).createGrant(recipient.address, { value: amount });
        const receipt = await tx.wait();
        const event = receipt.logs.find(l => l.fragment?.name === "GrantCreated");
        expect(event).to.exist;
    });

    it("rejects creating a grant for an unverified recipient", async () => {
        const amount = ethers.parseEther("1");
        await expect(
            grantStream.connect(funder).createGrant(unverified.address, { value: amount })
        ).to.be.revertedWith("GrantStream: recipient not KYC verified");
    });

    // ── claim with KYC ────────────────────────────────────────────────────────

    it("allows a KYC-verified recipient to claim", async () => {
        const amount = ethers.parseEther("1");
        const tx = await grantStream.connect(funder).createGrant(recipient.address, { value: amount });
        const receipt = await tx.wait();
        const event = receipt.logs.find(l => l.fragment?.name === "GrantCreated");
        const grantId = event.args.grantId;

        await expect(grantStream.connect(recipient).claim(grantId, amount)).to.not.be.reverted;
    });

    it("rejects claim if recipient's KYC is revoked after grant creation", async () => {
        const amount = ethers.parseEther("1");
        const tx = await grantStream.connect(funder).createGrant(recipient.address, { value: amount });
        const receipt = await tx.wait();
        const event = receipt.logs.find(l => l.fragment?.name === "GrantCreated");
        const grantId = event.args.grantId;

        // Verifier revokes KYC (e.g. fraud detected)
        await verifierContract.connect(verifierEOA).revokeVerification(recipient.address);

        await expect(
            grantStream.connect(recipient).claim(grantId, amount)
        ).to.be.revertedWith("GrantStream: recipient not KYC verified");
    });

    // ── KYC disabled ──────────────────────────────────────────────────────────

    it("allows grants for unverified recipients when KYC is disabled", async () => {
        await grantStream.connect(owner).setKYCRequired(false);

        const amount = ethers.parseEther("1");
        await expect(
            grantStream.connect(funder).createGrant(unverified.address, { value: amount })
        ).to.not.be.reverted;
    });
});

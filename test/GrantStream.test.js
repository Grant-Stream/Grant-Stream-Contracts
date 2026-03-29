const { expect } = require("chai");
const { ethers } = require("hardhat");

describe("GrantStream — Final_Protocol_Sustainability_Fund_Transfer", function () {
  let grantStream, fund;
  let owner, funder, recipient, treasury;

  const THRESHOLD = ethers.parseEther("100000"); // 100,000 ETH as proxy for $100k
  const TAX_BPS   = 100n;
  const BPS_DENOM = 1_000_000n;

  beforeEach(async () => {
    [owner, funder, recipient, treasury] = await ethers.getSigners();

    const Fund = await ethers.getContractFactory("SustainabilityFund");
    fund = await Fund.deploy(treasury.address);

    const GS = await ethers.getContractFactory("GrantStream");
    grantStream = await GS.deploy(await fund.getAddress());
  });

  // ── Helper ──────────────────────────────────────────────────────────────────
  async function createGrant(amount) {
    const tx = await grantStream.connect(funder).createGrant(recipient.address, { value: amount });
    const receipt = await tx.wait();
    const event = receipt.logs.find(l => l.fragment?.name === "GrantCreated");
    return event.args.grantId;
  }

  // ── Tests ───────────────────────────────────────────────────────────────────

  it("applies NO tax when volume stays below threshold", async () => {
    const deposit = ethers.parseEther("50000");
    const grantId = await createGrant(deposit);

    const before = await ethers.provider.getBalance(recipient.address);
    await grantStream.connect(recipient).claim(grantId, deposit);
    const after = await ethers.provider.getBalance(recipient.address);

    // Recipient should receive the full amount (minus gas)
    expect(after - before).to.be.closeTo(deposit, ethers.parseEther("0.01"));
    expect(await fund.totalReceived()).to.equal(0n);
  });

  it("applies 0.01% tax on the portion above threshold when claim straddles it", async () => {
    // Fund with exactly threshold so the claim crosses it
    const deposit = THRESHOLD + ethers.parseEther("1000");
    const grantId = await createGrant(deposit);

    // First claim brings volume to exactly THRESHOLD — no tax
    await grantStream.connect(recipient).claim(grantId, THRESHOLD);
    expect(await fund.totalReceived()).to.equal(0n);

    // Second claim of 1000 ETH — all above threshold, full tax applies
    const claimAmt = ethers.parseEther("1000");
    const expectedTax = (claimAmt * TAX_BPS) / BPS_DENOM;

    await grantStream.connect(recipient).claim(grantId, claimAmt);
    expect(await fund.totalReceived()).to.equal(expectedTax);
  });

  it("taxes only the above-threshold portion when a single claim straddles the threshold", async () => {
    const deposit = THRESHOLD + ethers.parseEther("500");
    const grantId = await createGrant(deposit);

    // Single claim that crosses the threshold
    const claimAmt = deposit; // 100_000.5 ETH total
    const aboveThreshold = ethers.parseEther("500");
    const expectedTax = (aboveThreshold * TAX_BPS) / BPS_DENOM;

    await grantStream.connect(recipient).claim(grantId, claimAmt);
    expect(await fund.totalReceived()).to.equal(expectedTax);
  });

  it("treasury can withdraw accumulated sustainability tax", async () => {
    const deposit = THRESHOLD + ethers.parseEther("1000");
    const grantId = await createGrant(deposit);

    await grantStream.connect(recipient).claim(grantId, THRESHOLD);
    const claimAmt = ethers.parseEther("1000");
    await grantStream.connect(recipient).claim(grantId, claimAmt);

    const accumulated = await fund.balance();
    expect(accumulated).to.be.gt(0n);

    const before = await ethers.provider.getBalance(treasury.address);
    await fund.connect(treasury).withdraw(0); // 0 = withdraw all
    const after = await ethers.provider.getBalance(treasury.address);

    expect(after - before).to.be.closeTo(accumulated, ethers.parseEther("0.001"));
    expect(await fund.balance()).to.equal(0n);
  });

  it("non-treasury address cannot withdraw", async () => {
    await expect(fund.connect(funder).withdraw(0))
      .to.be.revertedWith("SustainabilityFund: not treasury");
  });

  it("emits FundsClaimed with correct tax values", async () => {
    const deposit = THRESHOLD + ethers.parseEther("2000");
    const grantId = await createGrant(deposit);

    // Bring volume to threshold first
    await grantStream.connect(recipient).claim(grantId, THRESHOLD);

    const claimAmt = ethers.parseEther("2000");
    const expectedTax = (claimAmt * TAX_BPS) / BPS_DENOM;
    const expectedNet = claimAmt - expectedTax;

    await expect(grantStream.connect(recipient).claim(grantId, claimAmt))
      .to.emit(grantStream, "FundsClaimed")
      .withArgs(grantId, recipient.address, expectedNet, expectedTax);
  });
});

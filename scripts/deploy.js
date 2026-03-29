const { ethers } = require("hardhat");

async function main() {
  const [deployer] = await ethers.getSigners();
  console.log("Deploying with:", deployer.address);

  // In production, replace with the actual JerryIdoko Developer Treasury address
  const TREASURY_ADDRESS = process.env.TREASURY_ADDRESS || deployer.address;

  const SustainabilityFund = await ethers.getContractFactory("SustainabilityFund");
  const fund = await SustainabilityFund.deploy(TREASURY_ADDRESS);
  await fund.waitForDeployment();
  console.log("SustainabilityFund deployed to:", await fund.getAddress());

  const GrantStream = await ethers.getContractFactory("GrantStream");
  const grantStream = await GrantStream.deploy(await fund.getAddress());
  await grantStream.waitForDeployment();
  console.log("GrantStream deployed to:", await grantStream.getAddress());
}

main().catch((err) => {
  console.error(err);
  process.exitCode = 1;
});

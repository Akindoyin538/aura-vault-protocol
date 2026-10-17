use crate::errors::VaultError;
use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

pub const REQUIRED_SIGNATURES: u32 = 3;
pub const TIMELOCK_DURATION: u64 = 24 * 60 * 60; // 24 hours in seconds

#[contracttype]
#[derive(Clone, Debug)]
pub enum ProposalType {
    UpdateAdmin,
    UpdateUnderlyingToken,
    UpdateParameter(Symbol, i128),
}

#[contracttype]
#[derive(Clone, Debug)]
pub enum ProposalStatus {
    Pending,
    Approved,
    Executed,
    Rejected,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub proposal_type: ProposalType,
    pub proposer: Address,
    pub status: ProposalStatus,
    pub votes_for: u32,
    pub votes_against: u32,
    pub signers: Vec<Address>,
    pub created_at: u64,
    pub execution_time: u64,
}

#[contracttype]
pub enum GovDataKey {
    Signers,
    ProposalCount,
    Proposal(u64),
    ProposalVote(u64, Address),
    Admin,
}

pub fn get_signers(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&GovDataKey::Signers)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_signers(env: &Env, signers: &Vec<Address>) {
    env.storage().instance().set(&GovDataKey::Signers, signers);
}

pub fn get_proposal_count(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&GovDataKey::ProposalCount)
        .unwrap_or(0)
}

pub fn set_proposal_count(env: &Env, count: u64) {
    env.storage()
        .instance()
        .set(&GovDataKey::ProposalCount, &count);
}

pub fn get_proposal(env: &Env, id: u64) -> Option<Proposal> {
    env.storage()
        .instance()
        .get(&GovDataKey::Proposal(id))
}

pub fn set_proposal(env: &Env, id: u64, proposal: &Proposal) {
    env.storage()
        .instance()
        .set(&GovDataKey::Proposal(id), proposal);
}

pub fn has_voted(env: &Env, proposal_id: u64, signer: &Address) -> bool {
    let vote: Option<bool> = env
        .storage()
        .instance()
        .get(&GovDataKey::ProposalVote(proposal_id, signer.clone()));
    vote.is_some()
}

pub fn record_vote(env: &Env, proposal_id: u64, signer: &Address) {
    env.storage().instance().set(
        &GovDataKey::ProposalVote(proposal_id, signer.clone()),
        &true,
    );
}

pub fn initialize_governance(env: &Env, signers: Vec<Address>) -> Result<(), VaultError> {
    let current_signers = get_signers(env);
    if current_signers.len() > 0 {
        return Err(VaultError::AlreadyInitialized);
    }
    
    set_signers(env, &signers);
    set_proposal_count(env, 0);
    Ok(())
}

pub fn create_proposal(
    env: &Env,
    proposer: Address,
    proposal_type: ProposalType,
) -> Result<u64, VaultError> {
    proposer.require_auth();
    
    let signers = get_signers(env);
    if !signers.iter().any(|s| s == proposer) {
        return Err(VaultError::InvalidAddress);
    }

    let count = get_proposal_count(env);
    let new_id = count + 1;
    let current_time = env.ledger().timestamp();

    let proposal = Proposal {
        id: new_id,
        proposal_type,
        proposer,
        status: ProposalStatus::Pending,
        votes_for: 0,
        votes_against: 0,
        signers: Vec::new(env),
        created_at: current_time,
        execution_time: current_time + TIMELOCK_DURATION,
    };

    set_proposal(env, new_id, &proposal);
    set_proposal_count(env, new_id);

    Ok(new_id)
}

pub fn vote_on_proposal(
    env: &Env,
    voter: Address,
    proposal_id: u64,
    approve: bool,
) -> Result<(), VaultError> {
    voter.require_auth();

    let signers = get_signers(env);
    if !signers.iter().any(|s| s == voter) {
        return Err(VaultError::InvalidAddress);
    }

    let mut proposal = get_proposal(env, proposal_id)
        .ok_or(VaultError::NotInitialized)?;

    if has_voted(env, proposal_id, &voter) {
        return Err(VaultError::InvalidAddress); // Already voted
    }

    if matches!(proposal.status, ProposalStatus::Pending) {
        if approve {
            proposal.votes_for += 1;
        } else {
            proposal.votes_against += 1;
        }

        let mut signers_vec = proposal.signers.clone();
        signers_vec.push_back(voter.clone());
        proposal.signers = signers_vec;

        // Check if we have enough signatures
        if proposal.votes_for >= REQUIRED_SIGNATURES {
            proposal.status = ProposalStatus::Approved;
        }

        set_proposal(env, proposal_id, &proposal);
        record_vote(env, proposal_id, &voter);
    }

    Ok(())
}

pub fn execute_proposal(
    env: &Env,
    executor: Address,
    proposal_id: u64,
) -> Result<ProposalType, VaultError> {
    executor.require_auth();

    let mut proposal = get_proposal(env, proposal_id)
        .ok_or(VaultError::NotInitialized)?;

    // Verify execution time has passed
    let current_time = env.ledger().timestamp();
    if current_time < proposal.execution_time {
        return Err(VaultError::InvalidAddress); // Timelock not expired
    }

    if !matches!(proposal.status, ProposalStatus::Approved) {
        return Err(VaultError::InvalidAddress); // Not approved
    }

    proposal.status = ProposalStatus::Executed;
    set_proposal(env, proposal_id, &proposal);

    Ok(proposal.proposal_type)
}

pub fn get_proposal_status(env: &Env, proposal_id: u64) -> Option<ProposalStatus> {
    get_proposal(env, proposal_id).map(|p| p.status)
}

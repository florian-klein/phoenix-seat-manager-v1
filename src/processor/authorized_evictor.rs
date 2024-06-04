use phoenix::program::{
    checkers::{Program, Signer},
    system_utils::create_account,
};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_error::ProgramError,
    pubkey::Pubkey, rent::Rent, system_program, sysvar::Sysvar,
};

pub fn process_authorized_evictor(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    remove: bool,
) -> ProgramResult {
    let seat_manager_authority = Signer::new(&accounts[0])?;
    let authorized_delegate = &accounts[1];
    let authorized_delegate_pda = &accounts[2];

    // Get pubkey for PDA derived from: seat_manager_auth and authorized_delegate
    let authorized_delegate_pda_seeds = get_authorized_delegate_seeds_and_validate(
        &seat_manager_authority.key,
        &authorized_delegate.key,
        &authorized_delegate_pda.key,
        &program_id,
    )?;

    let system_program = Program::new(&accounts[3], &system_program::id())?;

    if !remove {
        create_account(
            &seat_manager_authority,
            &authorized_delegate_pda,
            &system_program,
            program_id,
            &Rent::get()?,
            0,
            authorized_delegate_pda_seeds,
        )?;
    } else {
        close_account(authorized_delegate_pda, seat_manager_authority.as_ref())?;
    }

    Ok(())
}

pub fn get_authorized_delegate_seeds_and_validate(
    seat_manager_authority: &Pubkey,
    authorized_delegate: &Pubkey,
    authorized_delegate_pda: &Pubkey,
    program_id: &Pubkey,
) -> Result<Vec<Vec<u8>>, ProgramError> {
    let mut seeds = vec![
        seat_manager_authority.to_bytes().to_vec(),
        authorized_delegate.to_bytes().to_vec(),
        b"delegate".to_vec(),
    ];
    let (derived_pda, bump) = Pubkey::find_program_address(
        seeds
            .iter()
            .map(|seed| seed.as_slice())
            .collect::<Vec<&[u8]>>()
            .as_slice(),
        &program_id,
    );
    seeds.push(vec![bump]);

    if derived_pda == *authorized_delegate_pda {
        Ok(seeds)
    } else {
        let caller = std::panic::Location::caller();
        msg!(
            "Invalid authorized delegate key, expected: {} found {}.\n{}",
            authorized_delegate_pda,
            derived_pda,
            caller
        );
        return Err(ProgramError::InvalidInstructionData.into());
    }
}

pub fn close_account<'info>(
    info: &AccountInfo<'info>,
    sol_destination: &AccountInfo<'info>,
) -> Result<(), ProgramError> {
    let dest_starting_lamports = sol_destination.lamports();

    **sol_destination.lamports.borrow_mut() =
        dest_starting_lamports.checked_add(info.lamports()).unwrap();

    **info.lamports.borrow_mut() = 0;

    info.assign(&system_program::ID);
    info.realloc(0, false).map_err(Into::into)
}

pub fn does_pda_exist(program_id: &Pubkey, pda_ai: &AccountInfo) -> bool {
    pda_ai.owner == program_id && pda_ai.lamports() != 0
}

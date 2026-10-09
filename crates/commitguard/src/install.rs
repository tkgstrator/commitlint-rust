//! Explicit native setup with isolated-home support and transactional activation.
use crate::Result;

mod activation;
mod assets;
mod command;
mod instructions;
mod paths;
mod preflight;
mod transaction;

pub fn run(args: &[String]) -> Result<()> {
    let ctx = preflight::prepare(args)?;
    let mut tx = transaction::Transaction::new(&ctx.home)?;
    if let Err(error) = activation::activate(&ctx, &mut tx) {
        if !tx.rollback() {
            return Err(format!(
                "{error}; rollback could not restore every managed file; inspect saved backups"
            ));
        }
        return Err(error);
    }
    if ctx.skills_only {
        println!("Native commit skills installed; no global Git guard activated.")
    } else {
        println!("Native commit policy installed. Open a new terminal for guarded Git on PATH.")
    }
    Ok(())
}

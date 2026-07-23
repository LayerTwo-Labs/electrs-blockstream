use elements::{confidential::Asset, DrivechainPeginData, PeginData, PegoutData, TxIn, TxOut};

use crate::chain::{bitcoin_genesis_hash, AssetId, BNetwork};
use crate::util::{FullHash, ScriptToAsm};

pub enum PeginKind<'a> {
    Standard(PeginData<'a>),
    Drivechain(DrivechainPeginData<'a>),
}

impl PeginKind<'_> {
    pub fn asset(&self) -> AssetId {
        match self {
            Self::Standard(pegin) => pegin.asset,
            Self::Drivechain(pegin) => pegin.asset,
        }
    }

    pub fn value(&self) -> u64 {
        match self {
            Self::Standard(pegin) => pegin.value,
            Self::Drivechain(pegin) => pegin.value,
        }
    }
}

pub fn get_pegin_data<'a>(
    txout: &'a TxIn,
    pegged_asset_id: Option<&AssetId>,
) -> Option<PeginKind<'a>> {
    let pegged_asset_id = pegged_asset_id?;
    if let Some(pegin) = txout.pegin_data().filter(|pegin| pegin.asset == *pegged_asset_id) {
        return Some(PeginKind::Standard(pegin));
    }
    txout
        .drivechain_pegin_data()
        .filter(|pegin| pegin.asset == *pegged_asset_id)
        .map(PeginKind::Drivechain)
}

#[derive(Serialize, Clone)]
pub struct DrivechainPeginValue {
    pub mainchain_txid: bitcoin::Txid,
    pub mainchain_vout: u32,
    pub value: u64,
    pub asset: AssetId,
    pub genesis_hash: bitcoin::BlockHash,
    pub claim_script: elements::Script,
}

impl DrivechainPeginValue {
    pub fn from_txin(txin: &TxIn, pegged_asset_id: Option<&AssetId>) -> Option<Self> {
        let pegged_asset_id = pegged_asset_id?;
        let pegin = txin
            .drivechain_pegin_data()
            .filter(|pegin| pegin.asset == *pegged_asset_id)?;
        Some(Self {
            mainchain_txid: pegin.mainchain_txid,
            mainchain_vout: pegin.outpoint.vout,
            value: pegin.value,
            asset: pegin.asset,
            genesis_hash: pegin.genesis_hash,
            claim_script: elements::Script::from(pegin.claim_script.to_vec()),
        })
    }
}

pub fn get_pegout_data<'a>(
    txout: &'a TxOut,
    pegged_asset_id: Option<&AssetId>,
    parent_network: BNetwork,
) -> Option<PegoutData<'a>> {
    let pegged_asset_id = pegged_asset_id?;
    txout.pegout_data().filter(|pegout| {
        pegout.asset == Asset::Explicit(*pegged_asset_id)
            && pegout.genesis_hash == bitcoin_genesis_hash(parent_network)
    })
}

// API representation of pegout data associated with an output
#[derive(Serialize, Clone)]
pub struct PegoutValue {
    pub genesis_hash: bitcoin::BlockHash,
    pub scriptpubkey: bitcoin::ScriptBuf,
    pub scriptpubkey_asm: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scriptpubkey_address: Option<bitcoin::Address>,
}

impl PegoutValue {
    pub fn from_txout(
        txout: &TxOut,
        pegged_asset_id: Option<&AssetId>,
        parent_network: BNetwork,
    ) -> Option<Self> {
        let pegoutdata = get_pegout_data(txout, pegged_asset_id, parent_network)?;

        let scriptpubkey = pegoutdata.script_pubkey;
        let address = bitcoin::Address::from_script(&scriptpubkey, parent_network).ok();

        Some(PegoutValue {
            genesis_hash: pegoutdata.genesis_hash,
            scriptpubkey_asm: scriptpubkey.to_asm(),
            scriptpubkey_address: address,
            scriptpubkey,
        })
    }
}

// Inner type for the indexer TxHistoryInfo::Pegin variant
#[derive(Serialize, Deserialize, Debug)]
pub struct PeginInfo {
    pub txid: FullHash,
    pub vin: u32,
    pub value: u64,
}

// Inner type for the indexer TxHistoryInfo::Pegout variant
#[derive(Serialize, Deserialize, Debug)]
pub struct PegoutInfo {
    pub txid: FullHash,
    pub vout: u32,
    pub value: u64,
}

#[cfg(test)]
mod tests {
    use super::{get_pegin_data, PeginKind};
    use bitcoin::hashes::Hash;
    use elements::{encode, AssetId, OutPoint, TxIn, TxInWitness, Txid};
    use std::str::FromStr;

    #[test]
    fn parses_drivechain_deposit_witness() {
        let mainchain_txid = bitcoin::Txid::from_str(
            "0101010101010101010101010101010101010101010101010101010101010101",
        )
        .unwrap();
        let asset = AssetId::from_str(
            "11b705a6eaa8ebcb8cbdb9ae162d415afd3ed787385e541dc3224b88c2089057",
        )
        .unwrap();
        let genesis = bitcoin::BlockHash::all_zeros();
        let mut txin = TxIn {
            previous_output: OutPoint::new(
                Txid::from_byte_array(mainchain_txid.to_byte_array()),
                0,
            ),
            is_pegin: true,
            witness: TxInWitness {
                pegin_witness: vec![
                    bitcoin::consensus::serialize(&100_000i64),
                    encode::serialize(&asset),
                    bitcoin::consensus::serialize(&genesis),
                    vec![0x51],
                    b"drivechain-deposit-v1".to_vec(),
                    mainchain_txid.to_byte_array().to_vec(),
                ],
                ..TxInWitness::default()
            },
            ..TxIn::default()
        };

        match get_pegin_data(&txin, Some(&asset)).expect("drivechain pegin") {
            PeginKind::Drivechain(pegin) => {
                assert_eq!(pegin.value, 100_000);
                assert_eq!(pegin.mainchain_txid, mainchain_txid);
            }
            PeginKind::Standard(_) => panic!("parsed drivechain witness as a standard pegin"),
        }

        txin.witness.pegin_witness[4] = b"wrong".to_vec();
        assert!(get_pegin_data(&txin, Some(&asset)).is_none());
    }
}

// Copyright (c) 2020 Intel Corporation
//
// SPDX-License-Identifier: BSD-2-Clause-Patent

use crate::error::{
    SpdmResult, SPDM_STATUS_ERROR_PEER, SPDM_STATUS_INVALID_MSG_FIELD,
    SPDM_STATUS_INVALID_PARAMETER,
};
use crate::message::*;
use crate::requester::*;

impl<'a> RequesterContext<'a> {
    pub fn send_receive_spdm_end_session(&mut self, session_id: u32) -> SpdmResult {
        info!("send spdm end_session\n");

        self.common.reset_buffer_via_request_code(
            SpdmRequestResponseCode::SpdmRequestEndSession,
            Some(session_id),
        );

        let mut send_buffer = [0u8; config::MAX_SPDM_MSG_SIZE];
        let used = self.encode_spdm_end_session(&mut send_buffer);
        self.send_secured_message(session_id, &send_buffer[..used], false)?;

        let mut receive_buffer = [0u8; config::MAX_SPDM_MSG_SIZE];
        let used = self.receive_secured_message(session_id, &mut receive_buffer, false)?;
        self.handle_spdm_end_session_response(session_id, &receive_buffer[..used])
    }

    pub fn encode_spdm_end_session(&mut self, buf: &mut [u8]) -> usize {
        let mut writer = Writer::init(buf);

        let request = SpdmMessage {
            header: SpdmMessageHeader {
                version: self.common.negotiate_info.spdm_version_sel,
                request_response_code: SpdmRequestResponseCode::SpdmRequestEndSession,
            },
            payload: SpdmMessagePayload::SpdmEndSessionRequest(SpdmEndSessionRequestPayload {
                end_session_request_attributes: SpdmEndSessionRequestAttributes::empty(),
            }),
        };
        if let Ok(sz) = request.spdm_encode(&mut self.common, &mut writer) {
            sz
        } else {
            0
        }
    }

    pub fn handle_spdm_end_session_response(
        &mut self,
        session_id: u32,
        receive_buffer: &[u8],
    ) -> SpdmResult {
        let mut reader = Reader::init(receive_buffer);
        match SpdmMessageHeader::read(&mut reader) {
            Some(message_header) => {
                if message_header.version != self.common.negotiate_info.spdm_version_sel {
                    return Err(SPDM_STATUS_INVALID_MSG_FIELD);
                }
                match message_header.request_response_code {
                    SpdmRequestResponseCode::SpdmResponseEndSessionAck => {
                        let end_session_rsp =
                            SpdmEndSessionResponsePayload::spdm_read(&mut self.common, &mut reader);
                        if let Some(end_session_rsp) = end_session_rsp {
                            debug!("!!! end_session rsp : {:02x?}\n", end_session_rsp);

                            let session =
                                if let Some(s) = self.common.get_session_via_id(session_id) {
                                    s
                                } else {
                                    return Err(SPDM_STATUS_INVALID_PARAMETER);
                                };
                            session.teardown(session_id)?;

                            Ok(())
                        } else {
                            error!("!!! end_session : fail !!!\n");
                            Err(SPDM_STATUS_INVALID_MSG_FIELD)
                        }
                    }
                    SpdmRequestResponseCode::SpdmResponseError => self
                        .spdm_handle_error_response_main(
                            Some(session_id),
                            receive_buffer,
                            SpdmRequestResponseCode::SpdmRequestEndSession,
                            SpdmRequestResponseCode::SpdmResponseEndSessionAck,
                        ),
                    _ => Err(SPDM_STATUS_ERROR_PEER),
                }
            }
            None => Err(SPDM_STATUS_INVALID_MSG_FIELD),
        }
    }
}

#[cfg(all(test,))]
mod tests_requester {
    use super::*;
    use crate::common::session::SpdmSession;
    use crate::responder;
    use crate::testlib::*;

    #[test]
    fn test_case0_send_receive_spdm_end_session() {
        let (rsp_config_info, rsp_provision_info) = create_info();
        let (req_config_info, req_provision_info) = create_info();

        let shared_buffer = SharedBuffer::new();
        let mut device_io_responder = FakeSpdmDeviceIoReceve::new(&shared_buffer);
        let pcidoe_transport_encap = &mut PciDoeTransportEncap {};

        crate::secret::asym_sign::register(SECRET_ASYM_IMPL_INSTANCE.clone());

        let mut responder = responder::ResponderContext::new(
            &mut device_io_responder,
            pcidoe_transport_encap,
            rsp_config_info,
            rsp_provision_info,
        );

        responder.common.negotiate_info.base_hash_sel = SpdmBaseHashAlgo::TPM_ALG_SHA_384;
        let rsp_session_id = 0xffu16;
        let session_id = (0xffu32 << 16) + rsp_session_id as u32;
        responder.common.session = gen_array_clone(SpdmSession::new(), 4);
        responder.common.session[0].setup(session_id).unwrap();
        responder.common.session[0].set_crypto_param(
            SpdmBaseHashAlgo::TPM_ALG_SHA_384,
            SpdmDheAlgo::SECP_384_R1,
            SpdmAeadAlgo::AES_256_GCM,
            SpdmKeyScheduleAlgo::SPDM_KEY_SCHEDULE,
        );
        responder.common.session[0]
            .set_session_state(crate::common::session::SpdmSessionState::SpdmSessionEstablished);

        let pcidoe_transport_encap2 = &mut PciDoeTransportEncap {};
        let mut device_io_requester = FakeSpdmDeviceIo::new(&shared_buffer, &mut responder);

        let mut requester = RequesterContext::new(
            &mut device_io_requester,
            pcidoe_transport_encap2,
            req_config_info,
            req_provision_info,
        );

        requester.common.negotiate_info.base_hash_sel = SpdmBaseHashAlgo::TPM_ALG_SHA_384;
        let rsp_session_id = 0xffu16;
        let session_id = (0xffu32 << 16) + rsp_session_id as u32;
        requester.common.session = gen_array_clone(SpdmSession::new(), 4);
        requester.common.session[0].setup(session_id).unwrap();
        requester.common.session[0].set_crypto_param(
            SpdmBaseHashAlgo::TPM_ALG_SHA_384,
            SpdmDheAlgo::SECP_384_R1,
            SpdmAeadAlgo::AES_256_GCM,
            SpdmKeyScheduleAlgo::SPDM_KEY_SCHEDULE,
        );
        requester.common.session[0]
            .set_session_state(crate::common::session::SpdmSessionState::SpdmSessionEstablished);

        let status = requester.end_session(session_id).is_ok();
        assert!(status);
    }
}

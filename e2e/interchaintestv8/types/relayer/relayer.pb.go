// Code generated by protoc-gen-go. DO NOT EDIT.
// versions:
// 	protoc-gen-go v1.36.6
// 	protoc        (unknown)
// source: relayer/relayer.proto

package relayer

import (
	protoreflect "google.golang.org/protobuf/reflect/protoreflect"
	protoimpl "google.golang.org/protobuf/runtime/protoimpl"
	reflect "reflect"
	sync "sync"
	unsafe "unsafe"
)

const (
	// Verify that this generated code is sufficiently up-to-date.
	_ = protoimpl.EnforceVersion(20 - protoimpl.MinVersion)
	// Verify that runtime/protoimpl is sufficiently up-to-date.
	_ = protoimpl.EnforceVersion(protoimpl.MaxVersion - 20)
)

// The request message
type RelayByTxRequest struct {
	state protoimpl.MessageState `protogen:"open.v1"`
	// The source chain identifier
	SrcChain string `protobuf:"bytes,1,opt,name=src_chain,json=srcChain,proto3" json:"src_chain,omitempty"`
	// The target chain identifier
	DstChain string `protobuf:"bytes,2,opt,name=dst_chain,json=dstChain,proto3" json:"dst_chain,omitempty"`
	// The identifiers for the IBC transactions to be relayed
	// This is usually the transaction hash
	SourceTxIds [][]byte `protobuf:"bytes,3,rep,name=source_tx_ids,json=sourceTxIds,proto3" json:"source_tx_ids,omitempty"`
	// The identifiers for the IBC transactions on the target chain to be timed out
	TimeoutTxIds [][]byte `protobuf:"bytes,4,rep,name=timeout_tx_ids,json=timeoutTxIds,proto3" json:"timeout_tx_ids,omitempty"`
	// The identifier for the source client
	// Used for event filtering
	SrcClientId string `protobuf:"bytes,5,opt,name=src_client_id,json=srcClientId,proto3" json:"src_client_id,omitempty"`
	// The identifier for the destination client
	// Used for event filtering
	DstClientId string `protobuf:"bytes,6,opt,name=dst_client_id,json=dstClientId,proto3" json:"dst_client_id,omitempty"`
	// The optional source chain send packet sequences for recv packets
	// Used for event filtering, no filtering if empty
	SrcPacketSequences []uint64 `protobuf:"varint,7,rep,packed,name=src_packet_sequences,json=srcPacketSequences,proto3" json:"src_packet_sequences,omitempty"`
	// The optional destination chain send packet sequences for acks and timeouts
	// Used for event filtering, no filtering if empty
	DstPacketSequences []uint64 `protobuf:"varint,8,rep,packed,name=dst_packet_sequences,json=dstPacketSequences,proto3" json:"dst_packet_sequences,omitempty"`
	unknownFields      protoimpl.UnknownFields
	sizeCache          protoimpl.SizeCache
}

func (x *RelayByTxRequest) Reset() {
	*x = RelayByTxRequest{}
	mi := &file_relayer_relayer_proto_msgTypes[0]
	ms := protoimpl.X.MessageStateOf(protoimpl.Pointer(x))
	ms.StoreMessageInfo(mi)
}

func (x *RelayByTxRequest) String() string {
	return protoimpl.X.MessageStringOf(x)
}

func (*RelayByTxRequest) ProtoMessage() {}

func (x *RelayByTxRequest) ProtoReflect() protoreflect.Message {
	mi := &file_relayer_relayer_proto_msgTypes[0]
	if x != nil {
		ms := protoimpl.X.MessageStateOf(protoimpl.Pointer(x))
		if ms.LoadMessageInfo() == nil {
			ms.StoreMessageInfo(mi)
		}
		return ms
	}
	return mi.MessageOf(x)
}

// Deprecated: Use RelayByTxRequest.ProtoReflect.Descriptor instead.
func (*RelayByTxRequest) Descriptor() ([]byte, []int) {
	return file_relayer_relayer_proto_rawDescGZIP(), []int{0}
}

func (x *RelayByTxRequest) GetSrcChain() string {
	if x != nil {
		return x.SrcChain
	}
	return ""
}

func (x *RelayByTxRequest) GetDstChain() string {
	if x != nil {
		return x.DstChain
	}
	return ""
}

func (x *RelayByTxRequest) GetSourceTxIds() [][]byte {
	if x != nil {
		return x.SourceTxIds
	}
	return nil
}

func (x *RelayByTxRequest) GetTimeoutTxIds() [][]byte {
	if x != nil {
		return x.TimeoutTxIds
	}
	return nil
}

func (x *RelayByTxRequest) GetSrcClientId() string {
	if x != nil {
		return x.SrcClientId
	}
	return ""
}

func (x *RelayByTxRequest) GetDstClientId() string {
	if x != nil {
		return x.DstClientId
	}
	return ""
}

func (x *RelayByTxRequest) GetSrcPacketSequences() []uint64 {
	if x != nil {
		return x.SrcPacketSequences
	}
	return nil
}

func (x *RelayByTxRequest) GetDstPacketSequences() []uint64 {
	if x != nil {
		return x.DstPacketSequences
	}
	return nil
}

// The response message
type RelayByTxResponse struct {
	state protoimpl.MessageState `protogen:"open.v1"`
	// The multicall transaction to be submitted by caller
	Tx []byte `protobuf:"bytes,1,opt,name=tx,proto3" json:"tx,omitempty"`
	// The contract address to submit the transaction, if applicable
	Address       string `protobuf:"bytes,2,opt,name=address,proto3" json:"address,omitempty"`
	unknownFields protoimpl.UnknownFields
	sizeCache     protoimpl.SizeCache
}

func (x *RelayByTxResponse) Reset() {
	*x = RelayByTxResponse{}
	mi := &file_relayer_relayer_proto_msgTypes[1]
	ms := protoimpl.X.MessageStateOf(protoimpl.Pointer(x))
	ms.StoreMessageInfo(mi)
}

func (x *RelayByTxResponse) String() string {
	return protoimpl.X.MessageStringOf(x)
}

func (*RelayByTxResponse) ProtoMessage() {}

func (x *RelayByTxResponse) ProtoReflect() protoreflect.Message {
	mi := &file_relayer_relayer_proto_msgTypes[1]
	if x != nil {
		ms := protoimpl.X.MessageStateOf(protoimpl.Pointer(x))
		if ms.LoadMessageInfo() == nil {
			ms.StoreMessageInfo(mi)
		}
		return ms
	}
	return mi.MessageOf(x)
}

// Deprecated: Use RelayByTxResponse.ProtoReflect.Descriptor instead.
func (*RelayByTxResponse) Descriptor() ([]byte, []int) {
	return file_relayer_relayer_proto_rawDescGZIP(), []int{1}
}

func (x *RelayByTxResponse) GetTx() []byte {
	if x != nil {
		return x.Tx
	}
	return nil
}

func (x *RelayByTxResponse) GetAddress() string {
	if x != nil {
		return x.Address
	}
	return ""
}

// Information request message
type InfoRequest struct {
	state protoimpl.MessageState `protogen:"open.v1"`
	// The source chain identifier
	SrcChain string `protobuf:"bytes,1,opt,name=src_chain,json=srcChain,proto3" json:"src_chain,omitempty"`
	// The target chain identifier
	DstChain      string `protobuf:"bytes,2,opt,name=dst_chain,json=dstChain,proto3" json:"dst_chain,omitempty"`
	unknownFields protoimpl.UnknownFields
	sizeCache     protoimpl.SizeCache
}

func (x *InfoRequest) Reset() {
	*x = InfoRequest{}
	mi := &file_relayer_relayer_proto_msgTypes[2]
	ms := protoimpl.X.MessageStateOf(protoimpl.Pointer(x))
	ms.StoreMessageInfo(mi)
}

func (x *InfoRequest) String() string {
	return protoimpl.X.MessageStringOf(x)
}

func (*InfoRequest) ProtoMessage() {}

func (x *InfoRequest) ProtoReflect() protoreflect.Message {
	mi := &file_relayer_relayer_proto_msgTypes[2]
	if x != nil {
		ms := protoimpl.X.MessageStateOf(protoimpl.Pointer(x))
		if ms.LoadMessageInfo() == nil {
			ms.StoreMessageInfo(mi)
		}
		return ms
	}
	return mi.MessageOf(x)
}

// Deprecated: Use InfoRequest.ProtoReflect.Descriptor instead.
func (*InfoRequest) Descriptor() ([]byte, []int) {
	return file_relayer_relayer_proto_rawDescGZIP(), []int{2}
}

func (x *InfoRequest) GetSrcChain() string {
	if x != nil {
		return x.SrcChain
	}
	return ""
}

func (x *InfoRequest) GetDstChain() string {
	if x != nil {
		return x.DstChain
	}
	return ""
}

// Information response message
type InfoResponse struct {
	state protoimpl.MessageState `protogen:"open.v1"`
	// The target chain information
	TargetChain *Chain `protobuf:"bytes,1,opt,name=target_chain,json=targetChain,proto3" json:"target_chain,omitempty"`
	// The source chain information
	SourceChain   *Chain `protobuf:"bytes,2,opt,name=source_chain,json=sourceChain,proto3" json:"source_chain,omitempty"`
	unknownFields protoimpl.UnknownFields
	sizeCache     protoimpl.SizeCache
}

func (x *InfoResponse) Reset() {
	*x = InfoResponse{}
	mi := &file_relayer_relayer_proto_msgTypes[3]
	ms := protoimpl.X.MessageStateOf(protoimpl.Pointer(x))
	ms.StoreMessageInfo(mi)
}

func (x *InfoResponse) String() string {
	return protoimpl.X.MessageStringOf(x)
}

func (*InfoResponse) ProtoMessage() {}

func (x *InfoResponse) ProtoReflect() protoreflect.Message {
	mi := &file_relayer_relayer_proto_msgTypes[3]
	if x != nil {
		ms := protoimpl.X.MessageStateOf(protoimpl.Pointer(x))
		if ms.LoadMessageInfo() == nil {
			ms.StoreMessageInfo(mi)
		}
		return ms
	}
	return mi.MessageOf(x)
}

// Deprecated: Use InfoResponse.ProtoReflect.Descriptor instead.
func (*InfoResponse) Descriptor() ([]byte, []int) {
	return file_relayer_relayer_proto_rawDescGZIP(), []int{3}
}

func (x *InfoResponse) GetTargetChain() *Chain {
	if x != nil {
		return x.TargetChain
	}
	return nil
}

func (x *InfoResponse) GetSourceChain() *Chain {
	if x != nil {
		return x.SourceChain
	}
	return nil
}

// The chain definition
type Chain struct {
	state protoimpl.MessageState `protogen:"open.v1"`
	// The chain id
	ChainId string `protobuf:"bytes,1,opt,name=chain_id,json=chainId,proto3" json:"chain_id,omitempty"`
	// The ibc version
	IbcVersion string `protobuf:"bytes,2,opt,name=ibc_version,json=ibcVersion,proto3" json:"ibc_version,omitempty"`
	// The ibc contract address
	IbcContract   string `protobuf:"bytes,3,opt,name=ibc_contract,json=ibcContract,proto3" json:"ibc_contract,omitempty"`
	unknownFields protoimpl.UnknownFields
	sizeCache     protoimpl.SizeCache
}

func (x *Chain) Reset() {
	*x = Chain{}
	mi := &file_relayer_relayer_proto_msgTypes[4]
	ms := protoimpl.X.MessageStateOf(protoimpl.Pointer(x))
	ms.StoreMessageInfo(mi)
}

func (x *Chain) String() string {
	return protoimpl.X.MessageStringOf(x)
}

func (*Chain) ProtoMessage() {}

func (x *Chain) ProtoReflect() protoreflect.Message {
	mi := &file_relayer_relayer_proto_msgTypes[4]
	if x != nil {
		ms := protoimpl.X.MessageStateOf(protoimpl.Pointer(x))
		if ms.LoadMessageInfo() == nil {
			ms.StoreMessageInfo(mi)
		}
		return ms
	}
	return mi.MessageOf(x)
}

// Deprecated: Use Chain.ProtoReflect.Descriptor instead.
func (*Chain) Descriptor() ([]byte, []int) {
	return file_relayer_relayer_proto_rawDescGZIP(), []int{4}
}

func (x *Chain) GetChainId() string {
	if x != nil {
		return x.ChainId
	}
	return ""
}

func (x *Chain) GetIbcVersion() string {
	if x != nil {
		return x.IbcVersion
	}
	return ""
}

func (x *Chain) GetIbcContract() string {
	if x != nil {
		return x.IbcContract
	}
	return ""
}

var File_relayer_relayer_proto protoreflect.FileDescriptor

const file_relayer_relayer_proto_rawDesc = "" +
	"\n" +
	"\x15relayer/relayer.proto\x12\arelayer\"\xc2\x02\n" +
	"\x10RelayByTxRequest\x12\x1b\n" +
	"\tsrc_chain\x18\x01 \x01(\tR\bsrcChain\x12\x1b\n" +
	"\tdst_chain\x18\x02 \x01(\tR\bdstChain\x12\"\n" +
	"\rsource_tx_ids\x18\x03 \x03(\fR\vsourceTxIds\x12$\n" +
	"\x0etimeout_tx_ids\x18\x04 \x03(\fR\ftimeoutTxIds\x12\"\n" +
	"\rsrc_client_id\x18\x05 \x01(\tR\vsrcClientId\x12\"\n" +
	"\rdst_client_id\x18\x06 \x01(\tR\vdstClientId\x120\n" +
	"\x14src_packet_sequences\x18\a \x03(\x04R\x12srcPacketSequences\x120\n" +
	"\x14dst_packet_sequences\x18\b \x03(\x04R\x12dstPacketSequences\"=\n" +
	"\x11RelayByTxResponse\x12\x0e\n" +
	"\x02tx\x18\x01 \x01(\fR\x02tx\x12\x18\n" +
	"\aaddress\x18\x02 \x01(\tR\aaddress\"G\n" +
	"\vInfoRequest\x12\x1b\n" +
	"\tsrc_chain\x18\x01 \x01(\tR\bsrcChain\x12\x1b\n" +
	"\tdst_chain\x18\x02 \x01(\tR\bdstChain\"t\n" +
	"\fInfoResponse\x121\n" +
	"\ftarget_chain\x18\x01 \x01(\v2\x0e.relayer.ChainR\vtargetChain\x121\n" +
	"\fsource_chain\x18\x02 \x01(\v2\x0e.relayer.ChainR\vsourceChain\"f\n" +
	"\x05Chain\x12\x19\n" +
	"\bchain_id\x18\x01 \x01(\tR\achainId\x12\x1f\n" +
	"\vibc_version\x18\x02 \x01(\tR\n" +
	"ibcVersion\x12!\n" +
	"\fibc_contract\x18\x03 \x01(\tR\vibcContract2\x89\x01\n" +
	"\x0eRelayerService\x12B\n" +
	"\tRelayByTx\x12\x19.relayer.RelayByTxRequest\x1a\x1a.relayer.RelayByTxResponse\x123\n" +
	"\x04Info\x12\x14.relayer.InfoRequest\x1a\x15.relayer.InfoResponseBf\n" +
	"\vcom.relayerB\fRelayerProtoP\x01Z\rtypes/relayer\xa2\x02\x03RXX\xaa\x02\aRelayer\xca\x02\aRelayer\xe2\x02\x13Relayer\\GPBMetadata\xea\x02\aRelayerb\x06proto3"

var (
	file_relayer_relayer_proto_rawDescOnce sync.Once
	file_relayer_relayer_proto_rawDescData []byte
)

func file_relayer_relayer_proto_rawDescGZIP() []byte {
	file_relayer_relayer_proto_rawDescOnce.Do(func() {
		file_relayer_relayer_proto_rawDescData = protoimpl.X.CompressGZIP(unsafe.Slice(unsafe.StringData(file_relayer_relayer_proto_rawDesc), len(file_relayer_relayer_proto_rawDesc)))
	})
	return file_relayer_relayer_proto_rawDescData
}

var file_relayer_relayer_proto_msgTypes = make([]protoimpl.MessageInfo, 5)
var file_relayer_relayer_proto_goTypes = []any{
	(*RelayByTxRequest)(nil),  // 0: relayer.RelayByTxRequest
	(*RelayByTxResponse)(nil), // 1: relayer.RelayByTxResponse
	(*InfoRequest)(nil),       // 2: relayer.InfoRequest
	(*InfoResponse)(nil),      // 3: relayer.InfoResponse
	(*Chain)(nil),             // 4: relayer.Chain
}
var file_relayer_relayer_proto_depIdxs = []int32{
	4, // 0: relayer.InfoResponse.target_chain:type_name -> relayer.Chain
	4, // 1: relayer.InfoResponse.source_chain:type_name -> relayer.Chain
	0, // 2: relayer.RelayerService.RelayByTx:input_type -> relayer.RelayByTxRequest
	2, // 3: relayer.RelayerService.Info:input_type -> relayer.InfoRequest
	1, // 4: relayer.RelayerService.RelayByTx:output_type -> relayer.RelayByTxResponse
	3, // 5: relayer.RelayerService.Info:output_type -> relayer.InfoResponse
	4, // [4:6] is the sub-list for method output_type
	2, // [2:4] is the sub-list for method input_type
	2, // [2:2] is the sub-list for extension type_name
	2, // [2:2] is the sub-list for extension extendee
	0, // [0:2] is the sub-list for field type_name
}

func init() { file_relayer_relayer_proto_init() }
func file_relayer_relayer_proto_init() {
	if File_relayer_relayer_proto != nil {
		return
	}
	type x struct{}
	out := protoimpl.TypeBuilder{
		File: protoimpl.DescBuilder{
			GoPackagePath: reflect.TypeOf(x{}).PkgPath(),
			RawDescriptor: unsafe.Slice(unsafe.StringData(file_relayer_relayer_proto_rawDesc), len(file_relayer_relayer_proto_rawDesc)),
			NumEnums:      0,
			NumMessages:   5,
			NumExtensions: 0,
			NumServices:   1,
		},
		GoTypes:           file_relayer_relayer_proto_goTypes,
		DependencyIndexes: file_relayer_relayer_proto_depIdxs,
		MessageInfos:      file_relayer_relayer_proto_msgTypes,
	}.Build()
	File_relayer_relayer_proto = out.File
	file_relayer_relayer_proto_goTypes = nil
	file_relayer_relayer_proto_depIdxs = nil
}

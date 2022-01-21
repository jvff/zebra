use super::Message;

/// A possibly incomplete network [`Message`].
///
/// This is the output of the [`Codec`][super::Codec]'s decoder implementation. It is either a
/// complete message or a marker to indicate that the message is still being downloaded.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum DecoderOutput {
    Complete(Message),
    Incomplete,
}

impl From<Message> for DecoderOutput {
    fn from(message: Message) -> Self {
        DecoderOutput::Complete(message)
    }
}

impl DecoderOutput {
    /// Retrieve the complete message if one was produced.
    pub fn into_complete_message(self) -> Option<Message> {
        match self {
            DecoderOutput::Complete(message) => Some(message),
            DecoderOutput::Incomplete => None,
        }
    }
}

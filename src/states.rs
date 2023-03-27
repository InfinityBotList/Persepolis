use strum_macros::{Display, EnumString};

#[derive(PartialEq, Display, EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum OnboardState {
    Pending, // Needed
    Started,
    QueueRemindedReviewer,
    Claimed,
    PendingManagerReview, // Needed
    Denied, // Needed
    Completed, // Needed
}
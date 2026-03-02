use super::{BrowseFocus, DoneFocus, InstallFocus, QueueFocus};

pub(super) fn queue_focus_label(focus: QueueFocus) -> &'static str {
    match focus {
        QueueFocus::Input => "Input",
        QueueFocus::Priority => "Priority",
        QueueFocus::Queue => "Queue",
        QueueFocus::Install => "Actions",
    }
}

pub(super) fn browse_focus_label(focus: BrowseFocus) -> &'static str {
    match focus {
        BrowseFocus::Search => "Search",
        BrowseFocus::Results => "Results",
    }
}

pub(super) fn install_focus_label(focus: InstallFocus) -> &'static str {
    match focus {
        InstallFocus::Progress => "Progress",
        InstallFocus::Current => "Current",
        InstallFocus::Logs => "Logs",
    }
}

pub(super) fn done_focus_label(focus: DoneFocus) -> &'static str {
    match focus {
        DoneFocus::Summary => "Summary",
        DoneFocus::Unresolved => "Unresolved",
    }
}

pub(super) fn next_queue_focus(focus: QueueFocus) -> QueueFocus {
    match focus {
        QueueFocus::Input => QueueFocus::Priority,
        QueueFocus::Priority => QueueFocus::Queue,
        QueueFocus::Queue => QueueFocus::Install,
        QueueFocus::Install => QueueFocus::Input,
    }
}

pub(super) fn prev_queue_focus(focus: QueueFocus) -> QueueFocus {
    match focus {
        QueueFocus::Input => QueueFocus::Install,
        QueueFocus::Priority => QueueFocus::Input,
        QueueFocus::Queue => QueueFocus::Priority,
        QueueFocus::Install => QueueFocus::Queue,
    }
}

pub(super) fn next_install_focus(focus: InstallFocus) -> InstallFocus {
    match focus {
        InstallFocus::Progress => InstallFocus::Current,
        InstallFocus::Current => InstallFocus::Logs,
        InstallFocus::Logs => InstallFocus::Progress,
    }
}

pub(super) fn prev_install_focus(focus: InstallFocus) -> InstallFocus {
    match focus {
        InstallFocus::Progress => InstallFocus::Logs,
        InstallFocus::Current => InstallFocus::Progress,
        InstallFocus::Logs => InstallFocus::Current,
    }
}

pub(super) fn next_done_focus(focus: DoneFocus) -> DoneFocus {
    match focus {
        DoneFocus::Summary => DoneFocus::Unresolved,
        DoneFocus::Unresolved => DoneFocus::Summary,
    }
}

pub(super) fn prev_done_focus(focus: DoneFocus) -> DoneFocus {
    next_done_focus(focus)
}

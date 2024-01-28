#[derive(Default, Debug, Copy, Clone)]
pub (crate) struct NetworkManager {
    pub (crate) weight:usize,
    pub (crate) started: bool,
    pub (crate) stopped: bool,
    pub (crate) debounce_end: bool,
    pub (crate) abort_debounce: bool
}

impl NetworkManager {
    pub (crate) fn update(&mut self, copy:NetworkManager) -> Self {
        self.weight = if copy.weight > self.weight {copy.weight}else{self.weight};
        self.started = copy.started;
        self.stopped = copy.stopped;
        self.debounce_end = copy.debounce_end;
        self.abort_debounce = copy.abort_debounce;

        self.clone()
    }
    pub fn new()-> NetworkManager {
        Self { weight: 0, started: false, stopped: true, debounce_end: false, abort_debounce: false }
    }

    pub fn is_running(&self) -> bool {
        self.weight > 0
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped
    }
    pub fn is_started(&self) -> bool {
        self.started
    }

    pub fn cancel_debounce(&mut self, abort: bool) {
        self.abort_debounce = abort;
    }

    pub fn check_if_finished(&mut self) {
        if self.is_running() {
            return;
        }
        self.cancel_debounce(false);

        while !self.debounce_end {
            if self.abort_debounce {
                break;
            }
          
            if self.is_running() {
                break;
            }

            self.started = false;
            self.debounce_end = true;
        }
    }
}
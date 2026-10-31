// A money transfer between two members, as it goes through review.
export type State = 'draft' | 'submitted' | 'approved' | 'rejected' | 'settled' | 'cancelled';

export class Transfer {
  state: State = 'draft';
  history: State[] = ['draft'];
  reason: string | null = null;

  private move(from: State[], to: State): void {
    if (!from.includes(this.state)) throw new Error(`cannot go from ${this.state} to ${to}`);
    this.state = to;
    this.history.push(to);
  }

  submit(): void {
    this.move(['draft'], 'submitted');
  }

  approve(): void {
    this.move(['submitted'], 'approved');
  }

  reject(reason: string): void {
    this.reason = reason;
    if (!reason.trim()) throw new Error('a rejection needs a reason');
    this.move(['submitted'], 'rejected');
  }

  settle(): void {
    this.move(['approved'], 'settled');
  }

  cancel(): void {
    this.move(['draft', 'submitted', 'approved'], 'cancelled');
  }
}

-- Small testbench used to produce the sample .ghw / .fst / .vcd files.
-- Regenerate with: examples/vhdl/generate.sh  (needs GHDL installed; tested with 6.0.0)
library ieee;
use ieee.std_logic_1164.all;
use ieee.numeric_std.all;

entity counter_tb is
end entity;

architecture sim of counter_tb is
  type state_t is (IDLE, COUNTING, WRAP);

  signal clk    : std_logic := '0';
  signal rst    : std_logic := '1';
  signal enable : std_logic := '0';
  signal count  : unsigned(7 downto 0) := (others => 'U');
  signal state  : state_t := IDLE;
  signal bus_oe : std_logic := 'Z';
begin
  clk <= not clk after 5 ns when now < 2 us;

  stimulus : process
  begin
    wait for 12 ns;
    rst <= '0';
    wait for 10 ns;
    enable <= '1';
    bus_oe <= '1';
    wait for 300 ns;
    bus_oe <= 'X';
    wait for 20 ns;
    bus_oe <= 'Z';
    wait;
  end process;

  counter : process (clk)
  begin
    if rising_edge(clk) then
      if rst = '1' then
        count <= (others => '0');
        state <= IDLE;
      elsif enable = '1' then
        count <= count + 1;
        if count = 255 then
          state <= WRAP;
        else
          state <= COUNTING;
        end if;
      end if;
    end if;
  end process;
end architecture;
